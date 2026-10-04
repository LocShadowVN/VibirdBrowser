use adblock::lists::{FilterFormat, ParseOptions};
use adblock::request::Request;
use adblock::Engine;
use shared::{ShieldLevel, ShieldVerdict};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, RwLock};
use std::thread;

const MAX_DOMAIN_RULES: usize = 8_000;
const MAX_SUBSTR_RULES: usize = 500;

const HARD_WHITELIST: &[&str] = &[
    "google.com",
    "googleapis.com",
    "gstatic.com",
    "googleusercontent.com",
    "googlevideo.com",
    "gvt1.com",
    "gvt2.com",
    "youtube.com",
    "ytimg.com",
    "ggpht.com",
    "youtu.be",
    "github.com",
    "githubusercontent.com",
    "githubassets.com",
    "gitlab.com",
    "stackoverflow.com",
    "stackexchange.com",
    "cloudflare.com",
    "cloudflare-dns.com",
    "jsdelivr.net",
    "unpkg.com",
    "cdnjs.cloudflare.com",
];

fn is_hard_whitelisted(domain: &str) -> bool {
    let d = domain.trim_start_matches('.').to_lowercase();
    for w in HARD_WHITELIST {
        if d == *w || d.ends_with(&format!(".{}", w)) {
            return true;
        }
    }
    false
}

enum ShieldJob {
    Check {
        url: String,
        host: String,
        reply_to: tokio::sync::oneshot::Sender<bool>,
    },
}

pub struct ShieldEngine {
    tx: Mutex<Sender<ShieldJob>>,
    level: RwLock<ShieldLevel>,
    blocked_count: AtomicU64,
    domain_blocks: RwLock<Vec<String>>,
    substr_blocks: RwLock<Vec<String>>,
    domain_whitelist: RwLock<Vec<String>>,
}

fn resolve_bundled_rules_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(data_dir) = dirs::data_local_dir() {
        candidates.push(data_dir.join("vibird-browser").join("custom_rules.txt"));
        candidates.push(data_dir.join("caram-browser").join("custom_rules.txt"));
    }

    if let Ok(appdir) = std::env::var("APPDIR") {
        let root = PathBuf::from(&appdir);
        candidates.push(root.join("usr/lib/vibird-browser/resources/rules.txt"));
        candidates.push(root.join("usr/lib/caram-browser/resources/rules.txt"));
        candidates.push(root.join("usr/lib/caram_browser/resources/rules.txt"));
        candidates.push(root.join("usr/bin/resources/rules.txt"));
        candidates.push(root.join("resources/rules.txt"));
    }

    candidates.push(PathBuf::from("/usr/lib/vibird-browser/resources/rules.txt"));
    candidates.push(PathBuf::from("/usr/lib/caram-browser/resources/rules.txt"));
    candidates.push(PathBuf::from("/usr/lib/caram_browser/resources/rules.txt"));
    candidates.push(PathBuf::from("/usr/share/vibird-browser/resources/rules.txt"));
    candidates.push(PathBuf::from("/usr/share/caram-browser/resources/rules.txt"));

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            candidates.push(parent.join("resources/rules.txt"));
            candidates.push(parent.join("../lib/vibird-browser/resources/rules.txt"));
            candidates.push(parent.join("../lib/caram-browser/resources/rules.txt"));
            candidates.push(parent.join("../lib/caram_browser/resources/rules.txt"));
        }
    }

    candidates.push(PathBuf::from("resources/rules.txt"));
    candidates.push(PathBuf::from("src-tauri/resources/rules.txt"));

    candidates.into_iter().find(|p| p.exists())
}

enum ParsedRule {
    DomainBlock(String),
    SubstrBlock(String),
    DomainWhitelist(String),
}

fn parse_rule(line: &str) -> Option<ParsedRule> {
    let mut s = line.trim();

    if s.is_empty() || s.starts_with('!') || s.starts_with('[') {
        return None;
    }

    let is_whitelist = s.starts_with("@@");
    if is_whitelist {
        s = &s[2..];
    }

    if let Some(idx) = s.find('$') {
        s = &s[..idx];
    }

    if s.len() > 2 && s.starts_with('/') && s.ends_with('/') {
        return None;
    }

    if let Some(rest) = s.strip_prefix("||") {
        let without_caret = rest.trim_end_matches('^');
        let domain = without_caret
            .split('/')
            .next()
            .unwrap_or(without_caret)
            .trim();
        if domain.is_empty() {
            return None;
        }
        if domain.contains('*') {
            return None;
        }
        if !domain.contains('.') && domain != "localhost" {
            return None;
        }
        if domain.len() > 128 {
            return None;
        }
        if !is_whitelist && is_hard_whitelisted(domain) {
            return None;
        }
        return Some(if is_whitelist {
            ParsedRule::DomainWhitelist(domain.to_string())
        } else {
            ParsedRule::DomainBlock(domain.to_string())
        });
    }

    if s.starts_with('*') && s.ends_with('*') && s.len() > 4 {
        let sub = &s[1..s.len() - 1];
        if !sub.contains('*') && sub.len() >= 6 && sub.len() <= 64 {
            if sub.chars().all(|c| c.is_ascii_graphic()) {
                if sub.contains('.') || sub.contains('/') || sub.contains('_') || sub.contains('-') {
                    return Some(if is_whitelist {
                        ParsedRule::DomainWhitelist(sub.to_string())
                    } else {
                        ParsedRule::SubstrBlock(sub.to_string())
                    });
                }
            }
        }
    }

    None
}

fn parse_rules_file(path: &PathBuf) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut domain_blocks = Vec::new();
    let mut substr_blocks = Vec::new();
    let mut whitelist = Vec::new();

    let Ok(content) = std::fs::read_to_string(path) else {
        return (domain_blocks, substr_blocks, whitelist);
    };

    for line in content.lines() {
        let trimmed = line.trim().trim_start_matches('\u{feff}');
        match parse_rule(trimmed) {
            Some(ParsedRule::DomainBlock(d)) => domain_blocks.push(d),
            Some(ParsedRule::SubstrBlock(s)) => substr_blocks.push(s),
            Some(ParsedRule::DomainWhitelist(w)) => whitelist.push(w),
            None => {}
        }
        if domain_blocks.len() >= MAX_DOMAIN_RULES && substr_blocks.len() >= MAX_SUBSTR_RULES {
            break;
        }
    }

    domain_blocks.truncate(MAX_DOMAIN_RULES);
    substr_blocks.truncate(MAX_SUBSTR_RULES);
    whitelist.truncate(MAX_SUBSTR_RULES);

    domain_blocks.sort();
    domain_blocks.dedup();
    substr_blocks.sort();
    substr_blocks.dedup();
    whitelist.sort();
    whitelist.dedup();

    (domain_blocks, substr_blocks, whitelist)
}

impl ShieldEngine {
    pub fn new() -> Self {
        let (tx, rx) = channel::<ShieldJob>();

        let resolved_path = resolve_bundled_rules_path();

        let (domain_blocks, substr_blocks, whitelist) = match resolved_path.as_ref() {
            Some(p) => parse_rules_file(p),
            None => (Vec::new(), Vec::new(), Vec::new()),
        };

        log::info!(
            "Vibird Shield: parsed {} domain blocks, {} substring blocks, {} whitelist",
            domain_blocks.len(),
            substr_blocks.len(),
            whitelist.len()
        );

        let path_for_worker = resolved_path.clone();
        let domain_blocks_for_state = domain_blocks.clone();
        let substr_blocks_for_state = substr_blocks.clone();
        let whitelist_for_state = whitelist.clone();

        thread::spawn(move || {
            let mut rules: Vec<String> = vec![
                "||doubleclick.net^$third-party".into(),
                "||googleadservices.com^".into(),
                "||pagead2.googlesyndication.com^".into(),
                "||adnxs.com^".into(),
                "||adroll.com^".into(),
                "||taboola.com^".into(),
                "||outbrain.com^".into(),
                "||criteo.com^".into(),
                "||hotjar.com^".into(),
                "||onetrust.com^".into(),
                "||cookielaw.org^".into(),
                "||cookiebot.com^".into(),
                "/ads/*".into(),
                "/adbanner/*".into(),
                "/telemetry/*".into(),
            ];

            let mut external_count = 0usize;

            if let Some(ref rules_path) = path_for_worker {
                match std::fs::read_to_string(rules_path) {
                    Ok(content) => {
                        for line in content.lines() {
                            let trimmed = line.trim().trim_start_matches('\u{feff}');
                            if !trimmed.is_empty()
                                && !trimmed.starts_with('!')
                                && !trimmed.starts_with('#')
                            {
                                rules.push(trimmed.to_string());
                                external_count += 1;
                            }
                        }
                        log::info!(
                            "Vibird Shield: loaded {} external rules from {:?}",
                            external_count,
                            rules_path
                        );
                    }
                    Err(e) => {
                        log::warn!("Vibird Shield: cannot read {:?}: {}", rules_path, e);
                    }
                }
            } else {
                log::warn!("Vibird Shield: no bundled rules.txt found");
            }

            log::info!(
                "Vibird Shield: engine initialized with {} total rules",
                rules.len()
            );

            let engine = Engine::from_rules(
                rules.iter().map(|s| s.as_str()),
                ParseOptions {
                    format: FilterFormat::Standard,
                    ..Default::default()
                },
            );

            while let Ok(job) = rx.recv() {
                match job {
                    ShieldJob::Check { url, host, reply_to } => {
                        let blocked = match Request::new(&url, &host, "script") {
                            Ok(req) => engine.check_network_request(&req).matched,
                            Err(_) => false,
                        };
                        let _ = reply_to.send(blocked);
                    }
                }
            }
        });

        Self {
            tx: Mutex::new(tx),
            level: RwLock::new(ShieldLevel::Standard),
            blocked_count: AtomicU64::new(0),
            domain_blocks: RwLock::new(domain_blocks_for_state),
            substr_blocks: RwLock::new(substr_blocks_for_state),
            domain_whitelist: RwLock::new(whitelist_for_state),
        }
    }

    pub fn set_level(&self, level: ShieldLevel) {
        if let Ok(mut l) = self.level.write() {
            *l = level;
        }
    }

    pub fn get_level(&self) -> ShieldLevel {
        self.level
            .read()
            .map(|l| l.clone())
            .unwrap_or(ShieldLevel::Standard)
    }

    pub fn get_blocked_count(&self) -> u64 {
        self.blocked_count.load(Ordering::Relaxed)
    }

    pub fn increment_blocked(&self, delta: u64) {
        self.blocked_count.fetch_add(delta, Ordering::Relaxed);
    }

    pub fn get_cosmetic_css(&self) -> &'static str {
        r#"
            .ad-banner, .adsbygoogle, [id^='google_ads_'], [id^='div-gpt-ad'],
            .ad-container, .ad-wrapper, .ad-slot, .ad_box, .advertisement,
            .sponsored-post, .taboola-ad, .outbrain-ad, [class*='sponsored'],
            [data-ad-client], [data-google-query-id], iframe[src*='doubleclick'],
            iframe[src*='adnxs'], .video-ads, .ytp-ad-module, .ytp-ad-overlay-container,
            .ytp-ad-overlay-slot, .ytp-ad-text-overlay, .ytp-ad-player-overlay,
            .ytp-ad-image-overlay, .ytp-ad-skip-button-container, .ytp-ad-progress-list,
            #player-ads, #masthead-ad, ytd-promoted-sparkles-web-renderer,
            ytd-display-ad-renderer, ytd-promoted-video-renderer,
            ytd-in-feed-ad-layout-renderer, ytd-ad-slot-renderer,
            ytd-banner-promo-renderer, ytd-statement-banner-renderer,
            #onetrust-consent-sdk, #onetrust-banner-sdk, .onetrust-pc-dark,
            #CybotCookiebotDialog, #CybotCookiebotDialogBody,
            .cc-window, .cc-banner, .cc-floating, .cc-dialog,
            #qc-cmp2-container, #qc-cmp2-ui,
            .cookie-banner, .cookie-notice, .cookie-consent, .cookie-popup,
            .cookie-policy-banner, [id*='cookie-notice'], [id*='cookiebanner'],
            [id*='cookie-law-info'], [id*='cookieConsent'], [class*='cookie-consent'],
            [class*='cookie-banner'], [class*='cookie-notice'], [class*='cookiebar'],
            [aria-label*='cookie' i], [aria-label*='consent' i],
            .fc-consent-root, .fc-dialog-overlay, .fc-dialog-container,
            #iubenda-cs-banner, .iubenda-cs-content,
            .cmp-container, #cmpbox, #cmpbox2,
            .sp_veil, .message-container, [id^='sp_message_container_'] {
                display: none !important;
                visibility: hidden !important;
                opacity: 0 !important;
                pointer-events: none !important;
                height: 0 !important;
                max-height: 0 !important;
                z-index: -99999 !important;
            }
        "#
    }
        pub fn get_injected_script(&self) -> String {
        let css = self.get_cosmetic_css();

        let domains_json = self
            .domain_blocks
            .read()
            .map(|v| serde_json::to_string(&*v).unwrap_or_else(|_| "[]".into()))
            .unwrap_or_else(|_| "[]".into());

        let subs_json = self
            .substr_blocks
            .read()
            .map(|v| serde_json::to_string(&*v).unwrap_or_else(|_| "[]".into()))
            .unwrap_or_else(|_| "[]".into());

        let wl_json = self
            .domain_whitelist
            .read()
            .map(|v| serde_json::to_string(&*v).unwrap_or_else(|_| "[]".into()))
            .unwrap_or_else(|_| "[]".into());

        let hard_wl_json = serde_json::to_string(HARD_WHITELIST).unwrap_or_else(|_| "[]".into());

        format!(
            r#"
            (function() {{
                'use strict';

                if (window.__VIBIRD_SHIELD_INSTALLED__) return;
                window.__VIBIRD_SHIELD_INSTALLED__ = true;

                // ============================================================
                // RULE DATA
                // ============================================================
                var VIBIRD_DOMAIN_BLOCK = new Set();
                var VIBIRD_SUBSTR_BLOCK = [];
                var VIBIRD_DOMAIN_WL = new Set();
                var VIBIRD_HARD_WL = new Set({hard_wl});
                var VIBIRD_LEGACY_PATTERNS = [
                    'doubleclick.net', 'googlesyndication.com',
                    'googleadservices.com', 'adnxs.com',
                    'adroll.com', 'taboola.com', 'outbrain.com', 'criteo.com',
                    'scorecardresearch.com', 'moatads.com',
                    'advertising.com', 'popads.net', 'amazon-adsystem.com',
                    'rubiconproject.com', 'openx.net', 'smartadserver.com',
                    'google-analytics.com', 'analytics.google.com', 'googletagmanager.com',
                    'hotjar.com', 'clarity.ms',
                    'facebook.com/tr',
                    'onetrust.com', 'cookielaw.org', 'cookiebot.com',
                    'tiktok.com/api/v1/pixel', 'bat.bing.com',
                    'youtube.com/api/stats/ads', 'youtube.com/pagead',
                    'youtube.com/ptracking', 'youtube.com/get_midroll_info',
                    'googleads.g.doubleclick.net', 'static.doubleclick.net',
                    'pubads.g.doubleclick.net',
                    'propellerads.com', 'popcash.net', 'popmyads.com',
                    'exoclick.com', 'juicyads.com', 'trafficjunky.com',
                    'clickadu.com', 'adsterra.com', 'hilltopads.net',
                    'onclickads.net', 'revcontent.com', 'mgid.com',
                    'zergnet.com', 'plista.com', 'sharethrough.com',
                    'teads.tv', 'spotxchange.com', 'bidswitch.net',
                    'adsrvr.org', 'casalemedia.com', '33across.com',
                    'sharethis.com', 'addthis.com', 'sumo.com',
                    'vungle.com', 'chartboost.com', 'applovin.com',
                    'unityads.unity3d.com', 'ironsrc.com', 'supersonicads.com',
                    'inmobi.com', 'mopub.com', 'fyber.com',
                    'serving-sys.com', 'sizmek.com', 'adform.net',
                    'flashtalking.com', 'simpli.fi', 'turn.com',
                    'mathtag.com', 'bluekai.com', 'demdex.net',
                    'krxd.net', 'rlcdn.com', 'agkn.com',
                    'adnxs-simple.com', 'adsafeprotected.com',
                    'moatpixel.com', 'doubleverify.com', 'iasds01.com',
                    'adsymptotic.com', 'semasio.net', 'zeotap.com',
                    'id5-sync.com', 'crwdcntrl.net', 'exelator.com',
                    'tapad.com', 'liadm.com', 'liveramp.com',
                    'ml314.com', 'matomo.cloud', 'statcounter.com',
                    'quantserve.com', 'quantcast.com', 'comscore.com',
                    'nielsen.com', 'imrworldwide.com', 'scorecardresearch.com',
                    'bugsnag.com', 'sentry.io', 'newrelic.com',
                    'logrocket.com', 'fullstory.com', 'smartlook.com',
                    'mouseflow.com', 'luckyorange.com', 'crazyegg.com',
                    'inspectlet.com', 'sessioncam.com', 'clicktale.net',
                    'yandex.ru/metrika', 'mc.yandex.ru', 'top-fwz1.mail.ru',
                    'baidu.com/hm.js', 'cnzz.com', 'umeng.com',
                    'talkingdata.com', 'growingio.com', 'sensorsdata.cn',
                    't.co/i/adsct', 'analytics.twitter.com',
                    'linkedin.com/px', 'snap.licdn.com',
                    'pinterest.com/ct', 'ct.pinterest.com',
                    'reddit.com/r/', 'redditstatic.com/ads',
                    'quora.com/_/ad', 'outbrain.com/widget',
                    'taboola.com/libtrc', 'criteo.net',
                    'criteo.com/delivery', 'adnxs.com/ttj',
                    'adnxs.com/jpt', 'adnxs.com/getuid',
                    'rubiconproject.com/a/api',
                    'pubmatic.com/AdServer',
                    'openx.net/w/1.0/arj',
                    'casalemedia.com/cygnus',
                    'agkn.com/pixel',
                    'mathtag.com/bid',
                    'advertising.com/pxrc',
                    'adsrvr.org/track',
                    'tapad.com/tap',
                    'demdex.net/dest',
                    'everesttech.net/everest',
                    'omtrdc.net/b/ss',
                    '2o7.net/b/ss',
                    '1rx.io/track',
                    'a-ads.com', 'cointraffic.io',
                    'popunderjs.com', 'popunder.net',
                    'adcash.com', 'zeropark.com',
                    'trafficstars.com', 'traffichaus.com',
                    'adspyglass.com', 'adspyder.com',
                    'adspyglass.com/ads',
                    'adreactor.com', 'adtelligent.com',
                    'advertising.com', 'advertising365.com',
                    'mydas.mobi', 'tremorhub.com',
                    'spotx.tv', 'spotxchange.com/video'
                ];

                try {{ VIBIRD_DOMAIN_BLOCK = new Set({domains}); }} catch (e) {{}}
                try {{ VIBIRD_SUBSTR_BLOCK = {subs}; }} catch (e) {{}}
                try {{ VIBIRD_DOMAIN_WL = new Set({wl}); }} catch (e) {{}}

                // ============================================================
                // TAURI INVOKE
                // ============================================================
                function vibirdInvoke(cmd, args) {{
                    try {{
                        if (window.__TAURI_INTERNALS__ && typeof window.__TAURI_INTERNALS__.invoke === 'function') {{
                            return window.__TAURI_INTERNALS__.invoke(cmd, args);
                        }}
                    }} catch (e) {{}}
                    try {{
                        if (window.__TAURI__ && window.__TAURI__.core && typeof window.__TAURI__.core.invoke === 'function') {{
                            return window.__TAURI__.core.invoke(cmd, args);
                        }}
                    }} catch (e) {{}}
                    return Promise.reject(new Error('No Tauri invoke available'));
                }}

                // ============================================================
                // BLOCK REPORTING (batch 500ms)
                // ============================================================
                var __pendingBlocks = 0;
                var __flushScheduled = false;
                window.__VIBIRD_BLOCKED_QUEUE = window.__VIBIRD_BLOCKED_QUEUE || 0;

                function tryInvoke(count) {{
                    try {{
                        vibirdInvoke('report_shield_block', {{ count: count }}).catch(function() {{}});
                        return true;
                    }} catch (e) {{
                        return false;
                    }}
                }}

                function flushBlocks() {{
                    __flushScheduled = false;
                    var count = __pendingBlocks;
                    __pendingBlocks = 0;
                    if (count <= 0) return;
                    if (!tryInvoke(count)) {{
                        window.__VIBIRD_BLOCKED_QUEUE += count;
                    }}
                }}

                function reportBlock() {{
                    __pendingBlocks++;
                    if (!__flushScheduled) {{
                        __flushScheduled = true;
                        setTimeout(flushBlocks, 500);
                    }}
                }}

                function flushQueue() {{
                    var pending = window.__VIBIRD_BLOCKED_QUEUE;
                    if (pending > 0 && tryInvoke(pending)) {{
                        window.__VIBIRD_BLOCKED_QUEUE = 0;
                    }}
                }}

                if (window.__VIBIRD_BLOCKED_QUEUE > 0) flushQueue();
                window.addEventListener('load', flushQueue, {{ once: true }});
                setTimeout(flushQueue, 100);
                setTimeout(flushQueue, 500);
                setTimeout(flushQueue, 2000);
                setTimeout(flushQueue, 5000);

                // ============================================================
                // URL MATCH
                // ============================================================
                function extractHost(url) {{
                    try {{
                        var u = new URL(url, location.href);
                        return u.hostname;
                    }} catch (e) {{
                        var m = url.match(/^https?:\/\/([^\/\?#]+)/i);
                        if (m) return m[1].split(':')[0];
                    }}
                    return '';
                }}

                function isInHardWhitelist(host) {{
                    if (!host) return false;
                    var parts = host.split('.');
                    for (var i = 0; i < parts.length; i++) {{
                        if (VIBIRD_HARD_WL.has(parts.slice(i).join('.'))) return true;
                    }}
                    return false;
                }}

                function hostnameMatches(host) {{
                    var parts = host.split('.');
                    var candidates = [];
                    for (var i = 0; i < parts.length; i++) {{
                        candidates.push(parts.slice(i).join('.'));
                    }}
                    for (var k = 0; k < candidates.length; k++) {{
                        if (VIBIRD_DOMAIN_WL.has(candidates[k])) return 'whitelist';
                    }}
                    for (var m = 0; m < candidates.length; m++) {{
                        if (VIBIRD_DOMAIN_BLOCK.has(candidates[m])) return 'block';
                    }}
                    return null;
                }}

                function isTrackingUrl(url) {{
                    if (!url || typeof url !== 'string') return false;

                    if (url.indexOf('data:') === 0 || url.indexOf('blob:') === 0) return false;
                    if (url.indexOf('about:') === 0) return false;

                    if (url.indexOf('/share') !== -1 || url.indexOf('/oauth') !== -1) return false;

                    var host = extractHost(url);

                    if (host && isInHardWhitelist(host)) {{
                        for (var l = 0; l < VIBIRD_LEGACY_PATTERNS.length; l++) {{
                            if (url.indexOf(VIBIRD_LEGACY_PATTERNS[l]) !== -1) return true;
                        }}
                        return false;
                    }}

                    if (host) {{
                        var result = hostnameMatches(host);
                        if (result === 'whitelist') {{
                            for (var l2 = 0; l2 < VIBIRD_LEGACY_PATTERNS.length; l2++) {{
                                if (url.indexOf(VIBIRD_LEGACY_PATTERNS[l2]) !== -1) return true;
                            }}
                            return false;
                        }}
                        if (result === 'block') return true;
                    }}

                    for (var i = 0; i < VIBIRD_SUBSTR_BLOCK.length; i++) {{
                        if (url.indexOf(VIBIRD_SUBSTR_BLOCK[i]) !== -1) return true;
                    }}

                    for (var j = 0; j < VIBIRD_LEGACY_PATTERNS.length; j++) {{
                        if (url.indexOf(VIBIRD_LEGACY_PATTERNS[j]) !== -1) return true;
                    }}

                    return false;
                }}

                // ============================================================
                // POPUP / WINDOW.OPEN BLOCK
                // ============================================================
                try {{
                    var __origWindowOpen = window.open;
                    window.open = function(url, name, features) {{
                        if (!url) {{
                            reportBlock();
                            return null;
                        }}
                        if (isTrackingUrl(url)) {{
                            reportBlock();
                            return null;
                        }}
                        return __origWindowOpen.call(window, url, name, features);
                    }};
                }} catch (e) {{}}

                // Chặn popup do click jacking - một số ad network mở popup
                // không qua window.open mà qua anchor target=_blank.
                try {{
                    document.addEventListener('click', function(e) {{
                        var t = e.target;
                        if (!t || !t.closest) return;
                        var a = t.closest('a[href]');
                        if (!a) return;
                        var href = a.getAttribute('href') || '';
                        var target = a.getAttribute('target') || '';
                        if (target === '_blank' && isTrackingUrl(href)) {{
                            e.preventDefault();
                            e.stopPropagation();
                            reportBlock();
                        }}
                    }}, true);
                }} catch (e) {{}}

                // ============================================================
                // ELEMENT SETTER HOOKS
                // ============================================================
                var origScriptSrcDesc = Object.getOwnPropertyDescriptor(HTMLScriptElement.prototype, 'src');
                if (origScriptSrcDesc) {{
                    Object.defineProperty(HTMLScriptElement.prototype, 'src', {{
                        set: function(val) {{
                            if (isTrackingUrl(val)) {{
                                reportBlock();
                                return origScriptSrcDesc.set.call(this, 'data:text/javascript,/*blocked-by-vibird-shield*/');
                            }}
                            return origScriptSrcDesc.set.call(this, val);
                        }},
                        get: function() {{ return origScriptSrcDesc.get.call(this); }}
                    }});
                }}

                var origIframeSrcDesc = Object.getOwnPropertyDescriptor(HTMLIFrameElement.prototype, 'src');
                if (origIframeSrcDesc) {{
                    Object.defineProperty(HTMLIFrameElement.prototype, 'src', {{
                        set: function(val) {{
                            if (isTrackingUrl(val)) {{
                                reportBlock();
                                return origIframeSrcDesc.set.call(this, 'about:blank');
                            }}
                            return origIframeSrcDesc.set.call(this, val);
                        }},
                        get: function() {{ return origIframeSrcDesc.get.call(this); }}
                    }});
                }}

                var origImgSrcDesc = Object.getOwnPropertyDescriptor(HTMLImageElement.prototype, 'src');
                if (origImgSrcDesc) {{
                    Object.defineProperty(HTMLImageElement.prototype, 'src', {{
                        set: function(val) {{
                            if (isTrackingUrl(val)) {{
                                reportBlock();
                                return origImgSrcDesc.set.call(this, 'data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 width=%221%22 height=%221%22/%3E');
                            }}
                            return origImgSrcDesc.set.call(this, val);
                        }},
                        get: function() {{ return origImgSrcDesc.get.call(this); }}
                    }});
                }}

                // ============================================================
                // SETATTRIBUTE HOOK
                // Bắt trường hợp script inject qua setAttribute('src', ...)
                // ============================================================
                try {{
                    var __origSetAttribute = Element.prototype.setAttribute;
                    Element.prototype.setAttribute = function(name, value) {{
                        try {{
                            if (name && typeof name === 'string') {{
                                var lname = name.toLowerCase();
                                if (lname === 'src' || lname === 'href' || lname === 'data-src' || lname === 'data-lazy-src') {{
                                    var tag = this.tagName ? this.tagName.toUpperCase() : '';
                                    if ((tag === 'SCRIPT' || tag === 'IFRAME' || tag === 'IMG' || tag === 'LINK' || tag === 'A') && typeof value === 'string' && isTrackingUrl(value)) {{
                                        reportBlock();
                                        if (tag === 'SCRIPT' || tag === 'LINK') {{
                                            // Không set attribute, script/link không load.
                                            return;
                                        }}
                                        if (tag === 'IFRAME') {{
                                            value = 'about:blank';
                                        }} else if (tag === 'IMG') {{
                                            value = 'data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 width=%221%22 height=%221%22/%3E';
                                        }}
                                    }}
                                }}
                            }}
                        }} catch (e) {{}}
                        return __origSetAttribute.call(this, name, value);
                    }};
                }} catch (e) {{}}

                // ============================================================
                // DOCUMENT.WRITE HOOK
                // Chặn script tag chèn qua document.write
                // ============================================================
                try {{
                    var __origDocWrite = document.write;
                    var __origDocWriteln = document.writeln;

                    function filterWriteArgs(args) {{
                        var out = [];
                        for (var i = 0; i < args.length; i++) {{
                            var s = String(args[i]);
                            if (s.indexOf('<script') !== -1 || s.indexOf('<iframe') !== -1) {{
                                // Trích xuất src từ HTML nếu có, rồi check.
                                var srcMatch = s.match(/(?:src|href)\s*=\s*["']([^"']+)["']/gi);
                                if (srcMatch) {{
                                    var blocked = false;
                                    for (var k = 0; k < srcMatch.length; k++) {{
                                        var url = srcMatch[k].replace(/^[^=]*=\s*["']/, '').replace(/["']$/, '');
                                        if (isTrackingUrl(url)) {{
                                            blocked = true;
                                            reportBlock();
                                            break;
                                        }}
                                    }}
                                    if (blocked) {{
                                        out.push('<!-- vibird-blocked -->');
                                        continue;
                                    }}
                                }}
                            }}
                            out.push(s);
                        }}
                        return out;
                    }}

                    document.write = function() {{
                        var args = filterWriteArgs(arguments);
                        return __origDocWrite.apply(document, args);
                    }};
                    document.writeln = function() {{
                        var args = filterWriteArgs(arguments);
                        return __origDocWriteln.apply(document, args);
                    }};
                }} catch (e) {{}}

                // ============================================================
                // INNERHTML HOOK (best-effort — chỉ bắt được setter, không bắt được
                // Framework-level render như React/Vue dùng createElement + append)
                // ============================================================
                try {{
                    var __origInnerHTMLDesc = Object.getOwnPropertyDescriptor(Element.prototype, 'innerHTML');
                    if (__origInnerHTMLDesc && __origInnerHTMLDesc.set) {{
                        Object.defineProperty(Element.prototype, 'innerHTML', {{
                            set: function(html) {{
                                try {{
                                    if (typeof html === 'string' && (html.indexOf('<script') !== -1 || html.indexOf('<iframe') !== -1)) {{
                                        var srcMatch = html.match(/(?:src|href)\s*=\s*["']([^"']+)["']/gi);
                                        if (srcMatch) {{
                                            var foundBlocked = false;
                                            for (var k = 0; k < srcMatch.length; k++) {{
                                                var url = srcMatch[k].replace(/^[^=]*=\s*["']/, '').replace(/["']$/, '');
                                                if (isTrackingUrl(url)) {{
                                                    reportBlock();
                                                    foundBlocked = true;
                                                }}
                                            }}
                                            if (foundBlocked) {{
                                                // Không xóa hết innerHTML, chỉ vệ sinh script/iframe ad.
                                                html = html.replace(/<script[^>]*src=["'][^"']*["'][^>]*>[\s\S]*?<\/script>/gi, function(m) {{
                                                    var u = m.match(/src=["']([^"']+)["']/i);
                                                    if (u && isTrackingUrl(u[1])) return '';
                                                    return m;
                                                }});
                                                html = html.replace(/<iframe[^>]*src=["'][^"']*["'][^>]*>[\s\S]*?<\/iframe>/gi, function(m) {{
                                                    var u = m.match(/src=["']([^"']+)["']/i);
                                                    if (u && isTrackingUrl(u[1])) return '';
                                                    return m;
                                                }});
                                            }}
                                        }}
                                    }}
                                }} catch (e) {{}}
                                return __origInnerHTMLDesc.set.call(this, html);
                            }},
                            get: function() {{ return __origInnerHTMLDesc.get.call(this); }}
                        }});
                    }}
                }} catch (e) {{}}

                // ============================================================
                // DOM SCAN — queue-based, throttle 200ms
                // ============================================================
                var __scanQueue = [];
                var __scanScheduled = false;
                var __scanRunning = false;

                function checkNode(node) {{
                    if (!node || node.nodeType !== 1) return;
                    var tag = node.tagName;
                    var url = '';
                    if (tag === 'IMG') url = node.src || node.getAttribute('src') || '';
                    else if (tag === 'SCRIPT') url = node.src || node.getAttribute('src') || '';
                    else if (tag === 'IFRAME') url = node.src || node.getAttribute('src') || '';
                    else if (tag === 'LINK') url = node.href || node.getAttribute('href') || '';

                    if (!url || !isTrackingUrl(url)) return;

                    try {{
                        if (tag === 'IMG') {{
                            node.src = 'data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 width=%221%22 height=%221%22/%3E';
                        }} else if (tag === 'SCRIPT') {{
                            node.type = 'javascript/blocked';
                            if (node.parentNode) node.parentNode.removeChild(node);
                        }} else if (tag === 'IFRAME') {{
                            node.src = 'about:blank';
                        }} else if (tag === 'LINK') {{
                            if (node.parentNode) node.parentNode.removeChild(node);
                        }}
                        reportBlock();
                    }} catch (e) {{}}
                }}

                function drainQueue() {{
                    __scanScheduled = false;
                    if (__scanRunning) return;
                    __scanRunning = true;
                    try {{
                        var limit = Math.min(__scanQueue.length, 500);
                        for (var i = 0; i < limit; i++) {{
                            var n = __scanQueue.shift();
                            checkNode(n);
                        }}
                    }} finally {{
                        __scanRunning = false;
                    }}
                    if (__scanQueue.length > 0) {{
                        __scanScheduled = true;
                        setTimeout(drainQueue, 50);
                    }}
                }}

                function enqueueScan(node) {{
                    if (!node) return;
                    __scanQueue.push(node);
                    if (__scanQueue.length > 5000) {{
                        __scanQueue = __scanQueue.slice(-2000);
                    }}
                    if (!__scanScheduled) {{
                        __scanScheduled = true;
                        setTimeout(drainQueue, 200);
                    }}
                }}

                function installDomObserver() {{
                    if (!document.body) {{
                        document.addEventListener('DOMContentLoaded', installDomObserver, {{ once: true }});
                        return;
                    }}
                    try {{
                        var observer = new MutationObserver(function(mutations) {{
                            for (var i = 0; i < mutations.length; i++) {{
                                var m = mutations[i];
                                if (m.type === 'childList' && m.addedNodes) {{
                                    for (var j = 0; j < m.addedNodes.length; j++) {{
                                        var n = m.addedNodes[j];
                                        if (n && n.nodeType === 1) {{
                                            enqueueScan(n);
                                            if (n.children && n.children.length > 0) {{
                                                for (var k = 0; k < n.children.length; k++) {{
                                                    enqueueScan(n.children[k]);
                                                }}
                                            }}
                                        }}
                                    }}
                                }}
                            }}
                        }});
                        observer.observe(document.documentElement, {{ childList: true, subtree: true }});
                    }} catch (e) {{}}
                }}
                installDomObserver();

                // ============================================================
                // WEBSOCKET
                // ============================================================
                var OrigWS = window.WebSocket;
                window.WebSocket = function(url, protocols) {{
                    if (isTrackingUrl(url)) {{
                        reportBlock();
                        throw new Error('Blocked by Vibird Shield');
                    }}
                    return new OrigWS(url, protocols);
                }};

                // ============================================================
                // FARBLING
                // ============================================================
                try {{
                    var origToDataURL = HTMLCanvasElement.prototype.toDataURL;
                    HTMLCanvasElement.prototype.toDataURL = function() {{
                        var ctx = this.getContext('2d');
                        if (ctx && this.width > 16 && this.height > 16) {{
                            try {{
                                var imgData = ctx.getImageData(0, 0, 2, 2);
                                imgData.data[0] = (imgData.data[0] ^ 1);
                                ctx.putImageData(imgData, 0, 0);
                            }} catch (e) {{}}
                        }}
                        return origToDataURL.apply(this, arguments);
                    }};

                    var origGetImageData = CanvasRenderingContext2D.prototype.getImageData;
                    CanvasRenderingContext2D.prototype.getImageData = function() {{
                        var res = origGetImageData.apply(this, arguments);
                        if (res && res.data && res.data.length > 4) {{
                            res.data[0] = (res.data[0] ^ 1);
                        }}
                        return res;
                    }};

                    if (window.AudioBuffer) {{
                        var origGetChannelData = AudioBuffer.prototype.getChannelData;
                        AudioBuffer.prototype.getChannelData = function() {{
                            var data = origGetChannelData.apply(this, arguments);
                            if (data && data.length > 0) {{
                                data[0] = data[0] + 0.00000001;
                            }}
                            return data;
                        }};
                    }}
                }} catch (e) {{}}

                // ============================================================
                // COOKIE DEFUSERS + SCRIPTLETS
                // ============================================================
                window.canRunAds = true;
                window.isAdBlockActive = false;
                window.ga = function() {{}};
                window.ga.q = [];
                window.gtag = function() {{}};
                window.fbq = function() {{}};
                window.dataLayer = window.dataLayer || [];
                window._paq = window._paq || [];
                window.piwik = window.piwik || {{}};
                window.piwik.getAsyncTracker = function() {{ return {{}}; }};
                window.mixpanel = window.mixpanel || {{}};
                window.mixpanel.track = function() {{}};
                window.amplitude = window.amplitude || {{}};
                window.amplitude.logEvent = function() {{}};
                window.heap = window.heap || {{}};
                window.heap.track = function() {{}};
                window.Intercom = function() {{}};
                window.hj = function() {{}};
                window._hsq = window._hsq || [];

                var stubCmp = function(cmd, ver, cb) {{
                    if (typeof cb === 'function') {{
                        cb({{ eventStatus: 'tcloaded', gdprApplies: false, tcString: '' }}, true);
                    }}
                }};
                window.__tcfapi = stubCmp;
                window.__cmp = stubCmp;
                window.__gpp = function() {{ return undefined; }};
                window.OneTrust = {{ IsAlertBoxClosed: function() {{ return true; }}, Close: function() {{}} }};
                window.Cookiebot = {{ consented: true, declined: false, hide: function() {{}} }};
                window.Optanon = {{ IsAlertBoxClosed: function() {{ return true; }} }};
                window.CookieConsent = window.CookieConsent || {{}};
                window.CookieConsent.acceptedCategory = function() {{ return true; }};
                window.CookieConsent.hasConsented = function() {{ return true; }};
                window.__uniconsent = {{ getConsentData: function(cb) {{ if (cb) cb('{{}}'); }} }};
                window.__uspapi = function(cmd, ver, cb) {{
                    if (typeof cb === 'function') cb({{ uspString: '1YNN' }}, true);
                }};

                if (navigator.sendBeacon) {{
                    var __origSendBeacon = navigator.sendBeacon.bind(navigator);
                    navigator.sendBeacon = function(url, data) {{
                        if (isTrackingUrl(url)) {{
                            reportBlock();
                            return true;
                        }}
                        return __origSendBeacon(url, data);
                    }};
                }}

                // Chặn legacy Google Analytics inject.
                try {{
                    var origCreateElement = document.createElement;
                    document.createElement = function(tag, options) {{
                        var el = origCreateElement.call(document, tag, options);
                        try {{
                            if (typeof tag === 'string' && tag.toLowerCase() === 'script') {{
                                // Observe khi src được set qua bất kỳ cách nào.
                                var origSrcDesc = Object.getOwnPropertyDescriptor(HTMLScriptElement.prototype, 'src');
                                if (origSrcDesc && origSrcDesc.set) {{
                                    Object.defineProperty(el, 'src', {{
                                        configurable: true,
                                        set: function(v) {{
                                            if (isTrackingUrl(v)) {{
                                                reportBlock();
                                                return;
                                            }}
                                            return origSrcDesc.set.call(this, v);
                                        }},
                                        get: function() {{ return origSrcDesc.get.call(this); }}
                                    }});
                                }}
                            }}
                        }} catch (e) {{}}
                        return el;
                    }};
                }} catch (e) {{}}

                // ============================================================
                // FETCH + XHR
                // ============================================================
                var origFetch = window.fetch;
                window.fetch = function(input, init) {{
                    var url = typeof input === 'string' ? input : (input && input.url ? input.url : '');
                    if (isTrackingUrl(url)) {{
                        reportBlock();
                        return Promise.resolve(new Response('', {{ status: 204, statusText: 'Blocked' }}));
                    }}
                    return origFetch.apply(this, arguments);
                }};

                var origOpen = XMLHttpRequest.prototype.open;
                XMLHttpRequest.prototype.open = function(method, url) {{
                    if (isTrackingUrl(url)) {{
                        reportBlock();
                        this.abort();
                        return;
                    }}
                    return origOpen.apply(this, arguments);
                }};

                // ============================================================
                // YOUTUBE AD SKIP
                // ============================================================
                function installYouTubeAdSkip() {{
                    if (!document.body) {{
                        document.addEventListener('DOMContentLoaded', installYouTubeAdSkip, {{ once: true }});
                        return;
                    }}
                    try {{
                        var ytObserver = new MutationObserver(function() {{
                            try {{
                                var skipBtn = document.querySelector('.ytp-ad-skip-button, .ytp-skip-ad-button, .ytp-ad-skip-button-modern');
                                if (skipBtn) {{ skipBtn.click(); reportBlock(); }}

                                var adVideo = document.querySelector('.ad-showing video, .video-ads video, .ytp-ad-player-overlay video');
                                if (adVideo && isFinite(adVideo.duration) && adVideo.duration > 0) {{
                                    try {{ adVideo.currentTime = adVideo.duration; reportBlock(); }} catch (e) {{}}
                                }}
                            }} catch (e) {{}}
                        }});
                        ytObserver.observe(document.body, {{ childList: true, subtree: true }});
                    }} catch (e) {{}}
                }}
                installYouTubeAdSkip();

                // ============================================================
                // AUTO-COLLAPSE EMPTY AD CONTAINERS
                // ============================================================
                function collapseEmptyAdContainers() {{
                    if (!document.body) return;
                    var kids = document.body.children;
                    for (var i = 0; i < kids.length && i < 10; i++) {{
                        var el = kids[i];
                        if (!el || el.nodeType !== 1) continue;
                        var cls = ((el.className || '') + ' ' + (el.id || '')).toLowerCase();
                        if (!/banner|sponsor|promo|ad[-_]?(?:container|slot|box|wrapper|skeleton)/.test(cls)) continue;
                        var rect = el.getBoundingClientRect();
                        if (rect.height < 150 || rect.top > 500) continue;
                        var text = (el.innerText || '').trim();
                        if (text.length > 0) continue;
                        var hasVisibleChild = false;
                        var ck = el.children;
                        for (var j = 0; j < ck.length; j++) {{
                            var cr = ck[j].getBoundingClientRect();
                            if (cr.height > 20 && cr.width > 20) {{ hasVisibleChild = true; break; }}
                        }}
                        if (hasVisibleChild) continue;
                        el.style.setProperty('display', 'none', 'important');
                    }}
                }}
                if (document.readyState === 'loading') {{
                    document.addEventListener('DOMContentLoaded', function() {{
                        setTimeout(collapseEmptyAdContainers, 1500);
                        setTimeout(collapseEmptyAdContainers, 4000);
                    }});
                }} else {{
                    setTimeout(collapseEmptyAdContainers, 1500);
                    setTimeout(collapseEmptyAdContainers, 4000);
                }}

                // ============================================================
                // COSMETIC CSS
                // ============================================================
                var injectCss = function() {{
                    if (document.getElementById('vibird-shield-cosmetics')) return;
                    var style = document.createElement('style');
                    style.id = 'vibird-shield-cosmetics';
                    style.textContent = `{css}`;
                    (document.head || document.documentElement).appendChild(style);
                }};
                if (document.readyState === 'loading') {{
                    document.addEventListener('DOMContentLoaded', injectCss);
                }} else {{
                    injectCss();
                }}
            }})();
            "#,
            hard_wl = hard_wl_json,
            domains = domains_json,
            subs = subs_json,
            wl = wl_json,
            css = css
        )
    }

    pub async fn inspect_url(&self, target_url: &str, host_url: &str) -> ShieldVerdict {
        let level = self.get_level();
        if level == ShieldLevel::Off {
            return ShieldVerdict {
                blocked: false,
                rule: None,
                level,
                cosmetic_css: String::new(),
            };
        }

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let sent = if let Ok(tx) = self.tx.lock() {
            tx.send(ShieldJob::Check {
                url: target_url.to_string(),
                host: host_url.to_string(),
                reply_to: reply_tx,
            })
            .is_ok()
        } else {
            false
        };

        let is_blocked = if sent {
            reply_rx.await.unwrap_or(false)
        } else {
            false
        };

        if is_blocked {
            self.blocked_count.fetch_add(1, Ordering::Relaxed);
        }

        ShieldVerdict {
            blocked: is_blocked,
            rule: if is_blocked {
                Some("Brave Engine Match".into())
            } else {
                None
            },
            level,
            cosmetic_css: self.get_cosmetic_css().to_string(),
        }
    }

    pub fn get_rule_counts(&self) -> (usize, usize, usize) {
        let d = self.domain_blocks.read().map(|v| v.len()).unwrap_or(0);
        let s = self.substr_blocks.read().map(|v| v.len()).unwrap_or(0);
        let w = self.domain_whitelist.read().map(|v| v.len()).unwrap_or(0);
        (d, s, w)
    }
}
