pub const CHROME_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/132.0.0.0 Safari/537.36";

// ============================================================================
// Spoofed Chrome version
// ----------------------------------------------------------------------------
// Bump khi UA decay ảnh hưởng đến compatibility với các site check version.
// Chrome release mỗi ~4 tuần; UA bị gate sau ~8-12 tháng.
// Lần bump gần nhất: 2025-01.
// ============================================================================
const SPOOFED_CHROME_MAJOR: u32 = 132;
const SPOOFED_CHROME_FULL: &str = "132.0.6834.83";

pub fn get_webbridge_script() -> String {
    format!(
        r#"
    (function() {{
        'use strict';

        if (window.__VIBIRD_BRIDGE_INJECTED) return;
        window.__VIBIRD_BRIDGE_INJECTED = true;

        var CHROME_MAJOR = "{major}";
        var CHROME_FULL = "{full}";
        var CHROME_UA = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{major}.0.0.0 Safari/537.36";

        // ================================================================
        // Native-code-preserving getter helper
        // ----------------------------------------------------------------
        // Khi override một property getter, một số anti-bot script sẽ check
        // `Object.getOwnPropertyDescriptor(navigator, 'x').get.toString()`
        // và expect chuỗi `[native code]`. Helper này fake đúng format đó.
        // ================================================================
        function makeNativeGetter(name, fn) {{
            var getter = function() {{ return fn.call(this); }};
            try {{
                Object.defineProperty(getter, 'name', {{ value: 'get ' + name, configurable: true }});
            }} catch (e) {{}}
            var fakeNative = 'function get ' + name + '() {{ [native code] }}';
            var origToString = Function.prototype.toString;
            getter.toString = function() {{ return fakeNative; }};
            return getter;
        }}

        function safeDefine(obj, key, value) {{
            try {{
                Object.defineProperty(obj, key, {{
                    get: makeNativeGetter(key, function() {{ return value; }}),
                    configurable: true,
                }});
            }} catch (e) {{}}
        }}

        // ================================================================
        // 1. WebRTC IP LEAK SHIELD
        // ----------------------------------------------------------------
        // Loại bỏ địa chỉ IP nội bộ (RFC 1918, link-local IPv6) khỏi SDP
        // candidate và intercept sự kiện onicecandidate để chặn rò rỉ.
        // ================================================================
        try {{
            if (window.RTCPeerConnection) {{
                var OrigPeerConnection = window.RTCPeerConnection;

                function sanitizeCandidate(candStr) {{
                    if (!candStr || typeof candStr !== 'string') return candStr;
                    var privateIpRegex = /(?:192\.168\.\d{{1,3}}\.\d{{1,3}}|10\.\d{{1,3}}\.\d{{1,3}}\.\d{{1,3}}|172\.(?:1[6-9]|2\d|3[0-1])\.\d{{1,3}}\.\d{{1,3}}|fe80::[0-9a-fA-F:]+)/;
                    if (candStr.indexOf('typ host') !== -1 && privateIpRegex.test(candStr)) {{
                        return null;
                    }}
                    return candStr;
                }}

                function sanitizeSdp(sdpStr) {{
                    if (!sdpStr || typeof sdpStr !== 'string') return sdpStr;
                    return sdpStr.split('\r\n').filter(function(line) {{
                        if (line.indexOf('a=candidate:') === 0 && line.indexOf('typ host') !== -1) {{
                            var privateIpRegex = /(?:192\.168\.\d{{1,3}}\.\d{{1,3}}|10\.\d{{1,3}}\.\d{{1,3}}\.\d{{1,3}}|172\.(?:1[6-9]|2\d|3[0-1])\.\d{{1,3}}\.\d{{1,3}})/;
                            return !privateIpRegex.test(line);
                        }}
                        return true;
                    }}).join('\r\n');
                }}

                window.RTCPeerConnection = function(config, constraints) {{
                    var pc = new OrigPeerConnection(config, constraints);

                    var origCreateOffer = pc.createOffer.bind(pc);
                    pc.createOffer = function(options) {{
                        return origCreateOffer(options).then(function(offer) {{
                            offer.sdp = sanitizeSdp(offer.sdp);
                            return offer;
                        }});
                    }};

                    var origCreateAnswer = pc.createAnswer.bind(pc);
                    pc.createAnswer = function(options) {{
                        return origCreateAnswer(options).then(function(answer) {{
                            answer.sdp = sanitizeSdp(answer.sdp);
                            return answer;
                        }});
                    }};

                    var userIceHandler = null;
                    var wrappedIceHandler = null;
                    Object.defineProperty(pc, 'onicecandidate', {{
                        set: function(fn) {{
                            userIceHandler = fn;
                            if (wrappedIceHandler) {{
                                pc.removeEventListener('icecandidate', wrappedIceHandler);
                            }}
                            wrappedIceHandler = function(e) {{
                                if (e.candidate && e.candidate.candidate) {{
                                    var sanitized = sanitizeCandidate(e.candidate.candidate);
                                    if (!sanitized) {{
                                        e.stopImmediatePropagation();
                                        return;
                                    }}
                                }}
                                if (typeof userIceHandler === 'function') {{
                                    userIceHandler.apply(pc, arguments);
                                }}
                            }};
                            pc.addEventListener('icecandidate', wrappedIceHandler);
                        }},
                        get: function() {{ return userIceHandler; }},
                        configurable: true,
                    }});

                    return pc;
                }};

                window.RTCPeerConnection.prototype = OrigPeerConnection.prototype;
            }}
        }} catch (e) {{}}

        // ================================================================
        // 2. NAVIGATOR SPOOFING (UA, platform, client hints)
        // ================================================================
        try {{
            safeDefine(navigator, 'userAgent', CHROME_UA);
            safeDefine(navigator, 'appVersion', CHROME_UA.replace('Mozilla/', ''));
            safeDefine(navigator, 'platform', 'Linux x86_64');
            safeDefine(navigator, 'vendor', 'Google Inc.');
            safeDefine(navigator, 'vendorSub', '');
            safeDefine(navigator, 'productSub', '20030107');
            safeDefine(navigator, 'webdriver', false);
            safeDefine(navigator, 'deviceMemory', 8);
            safeDefine(navigator, 'pdfViewerEnabled', true);

            var fullVersionList = [
                {{ brand: 'Chromium', version: CHROME_MAJOR }},
                {{ brand: 'Not)A;Brand', version: '24' }},
                {{ brand: 'Google Chrome', version: CHROME_MAJOR }}
            ];
            var greaseBrands = [
                {{ brand: 'Chromium', version: CHROME_MAJOR }},
                {{ brand: 'Not)A;Brand', version: '24' }},
                {{ brand: 'Google Chrome', version: CHROME_MAJOR }}
            ];

            var uaData = {{
                brands: greaseBrands,
                mobile: false,
                platform: 'Linux',
                getHighEntropyValues: function(hints) {{
                    return Promise.resolve({{
                        architecture: 'x86',
                        bitness: '64',
                        brands: greaseBrands,
                        fullVersionList: [
                            {{ brand: 'Chromium', version: CHROME_FULL }},
                            {{ brand: 'Not)A;Brand', version: '24.0.0.0' }},
                            {{ brand: 'Google Chrome', version: CHROME_FULL }}
                        ],
                        mobile: false,
                        model: '',
                        platform: 'Linux',
                        platformVersion: '6.8.0',
                        uaFullVersion: CHROME_FULL,
                        wow64: false
                    }});
                }},
                toJSON: function() {{
                    return {{ brands: this.brands, mobile: this.mobile, platform: this.platform }};
                }}
            }};
            safeDefine(navigator, 'userAgentData', uaData);
        }} catch (e) {{}}

        // ================================================================
        // 3. NAVIGATOR.PLUGINS + MIMETYPES
        // ----------------------------------------------------------------
        // Chrome Linux desktop luôn report 5 PDF plugin entries. WebKitGTK
        // report khác (thường rỗng hoặc 1-2) nên đây là fingerprint rõ ràng.
        // Fake đúng set của Chrome để indistinguish.
        // ================================================================
        try {{
            function makePlugin(name, desc, filename) {{
                var mimeEntry = {{
                    type: 'application/pdf',
                    suffixes: 'pdf',
                    description: desc,
                    enabledPlugin: null
                }};
                var plugin = {{
                    0: mimeEntry,
                    name: name,
                    description: desc,
                    filename: filename,
                    length: 1,
                    item: function(i) {{ return i === 0 ? mimeEntry : null; }},
                    namedItem: function(n) {{ return n === 'application/pdf' ? mimeEntry : null; }}
                }};
                mimeEntry.enabledPlugin = plugin;
                return plugin;
            }}

            var pluginNames = [
                ['PDF Viewer', 'Portable Document Format', 'internal-pdf-viewer'],
                ['Chrome PDF Viewer', 'Portable Document Format', 'internal-pdf-viewer'],
                ['Chromium PDF Viewer', 'Portable Document Format', 'internal-pdf-viewer'],
                ['Microsoft Edge PDF Viewer', 'Portable Document Format', 'internal-pdf-viewer'],
                ['WebKit built-in PDF', 'Portable Document Format', 'internal-pdf-viewer']
            ];

            var pluginArr = {{ length: pluginNames.length }};
            for (var pi = 0; pi < pluginNames.length; pi++) {{
                pluginArr[pi] = makePlugin(pluginNames[pi][0], pluginNames[pi][1], pluginNames[pi][2]);
            }}
            pluginArr.item = function(i) {{ return pluginArr[i] || null; }};
            pluginArr.namedItem = function(n) {{
                for (var j = 0; j < pluginNames.length; j++) {{
                    if (pluginNames[j][0] === n) return pluginArr[j];
                }}
                return null;
            }};
            pluginArr[Symbol.iterator] = function*() {{
                for (var k = 0; k < pluginNames.length; k++) yield pluginArr[k];
            }};

            var mimeArr = {{
                0: {{ type: 'application/pdf', suffixes: 'pdf', description: 'Portable Document Format', enabledPlugin: pluginArr[0] }},
                1: {{ type: 'text/pdf', suffixes: 'pdf', description: 'Portable Document Format', enabledPlugin: pluginArr[0] }},
                length: 2
            }};
            mimeArr.item = function(i) {{ return mimeArr[i] || null; }};
            mimeArr.namedItem = function(n) {{
                if (n === 'application/pdf') return mimeArr[0];
                if (n === 'text/pdf') return mimeArr[1];
                return null;
            }};
            mimeArr[Symbol.iterator] = function*() {{
                yield mimeArr[0];
                yield mimeArr[1];
            }};

            safeDefine(navigator, 'plugins', pluginArr);
            safeDefine(navigator, 'mimeTypes', mimeArr);
        }} catch (e) {{}}

        // ================================================================
        // 4. WINDOW.CHROME RUNTIME POLYFILL
        // ================================================================
        try {{
            if (!window.chrome) window.chrome = {{}};

            window.chrome.app = {{
                isInstalled: false,
                InstallState: {{ DISABLED: 'disabled', INSTALLED: 'installed', NOT_INSTALLED: 'not_installed' }},
                RunningState: {{ CANNOT_RUN: 'cannot_run', READY_TO_RUN: 'ready_to_run', RUNNING: 'running' }},
                getDetails: function() {{ return null; }},
                getIsInstalled: function() {{ return false; }},
                runningState: function() {{ return 'cannot_run'; }}
            }};

            window.chrome.csi = function() {{
                var now = performance.now();
                return {{ startE: now, onloadT: now, pageT: now, tran: 15 }};
            }};

            window.chrome.loadTimes = function() {{
                var nowSec = performance.now() / 1000;
                return {{
                    requestTime: nowSec,
                    startLoadTime: nowSec,
                    commitLoadTime: nowSec,
                    finishDocumentLoadTime: nowSec,
                    finishLoadTime: nowSec,
                    firstPaintTime: nowSec,
                    firstPaintAfterLoadTime: 0,
                    navigationType: 'Other',
                    wasFetchedViaSpdy: true,
                    wasNpnNegotiated: true,
                    npnNegotiatedProtocol: 'h2',
                    wasAlternateProtocolAvailable: false,
                    connectionInfo: 'h2'
                }};
            }};

            if (!window.chrome.runtime) {{
                window.chrome.runtime = {{
                    id: undefined,
                    connect: function() {{
                        return {{
                            onMessage: {{ addListener: function() {{}}, removeListener: function() {{}} }},
                            onDisconnect: {{ addListener: function() {{}}, removeListener: function() {{}} }},
                            postMessage: function() {{}}
                        }};
                    }},
                    sendMessage: function(extensionId, message, options, responseCallback) {{
                        var cb = typeof options === 'function' ? options : responseCallback;
                        if (cb) setTimeout(function() {{ cb(); }}, 0);
                    }},
                    getManifest: function() {{ return null; }}
                }};
            }}
        }} catch (e) {{}}

        // ================================================================
        // 5. WEBRTC & MEDIADEVICES CONSTRAINTS SHIM
        // ================================================================
        try {{
            if (window.MediaStreamTrack && !MediaStreamTrack.prototype.getCapabilities) {{
                MediaStreamTrack.prototype.getCapabilities = function() {{
                    return {{
                        aspectRatio: {{ max: 1920, min: 0.00052 }},
                        facingMode: ['user', 'environment'],
                        frameRate: {{ max: 60, min: 1 }},
                        height: {{ max: 1080, min: 1 }},
                        width: {{ max: 1920, min: 1 }},
                        deviceId: 'default',
                        groupId: 'default'
                    }};
                }};
            }}

            function makeRtpCaps(kind) {{
                if (kind === 'video') {{
                    return {{
                        codecs: [
                            {{ mimeType: 'video/VP8', clockRate: 90000, channels: undefined, sdpFmtpLine: undefined }},
                            {{ mimeType: 'video/rtx', clockRate: 90000, sdpFmtpLine: 'apt=96' }},
                            {{ mimeType: 'video/VP9', clockRate: 90000, sdpFmtpLine: 'profile-id=0' }},
                            {{ mimeType: 'video/VP9', clockRate: 90000, sdpFmtpLine: 'profile-id=2' }},
                            {{ mimeType: 'video/H264', clockRate: 90000, sdpFmtpLine: 'level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f' }},
                            {{ mimeType: 'video/rtx', clockRate: 90000, sdpFmtpLine: 'apt=98' }},
                            {{ mimeType: 'video/AV1', clockRate: 90000 }},
                            {{ mimeType: 'video/red', clockRate: 90000 }},
                            {{ mimeType: 'video/ulpfec', clockRate: 90000 }}
                        ],
                        headerExtensions: [
                            {{ uri: 'urn:ietf:params:rtp-hdrext:toffset', preferredId: 14 }},
                            {{ uri: 'http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time', preferredId: 4 }},
                            {{ uri: 'urn:3gpp:video-orientation', preferredId: 13 }}
                        ]
                    }};
                }}
                return {{
                    codecs: [
                        {{ mimeType: 'audio/opus', clockRate: 48000, channels: 2 }},
                        {{ mimeType: 'audio/ISAC', clockRate: 16000 }},
                        {{ mimeType: 'audio/G722', clockRate: 8000 }},
                        {{ mimeType: 'audio/PCMU', clockRate: 8000 }},
                        {{ mimeType: 'audio/PCMA', clockRate: 8000 }},
                        {{ mimeType: 'audio/CN', clockRate: 32000 }},
                        {{ mimeType: 'audio/telephone-event', clockRate: 8000 }},
                        {{ mimeType: 'audio/red', clockRate: 48000 }},
                        {{ mimeType: 'audio/rtx', clockRate: 8000 }}
                    ],
                    headerExtensions: [
                        {{ uri: 'urn:ietf:params:rtp-hdrext:ssrc-audio-level', preferredId: 1 }},
                        {{ uri: 'http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time', preferredId: 4 }},
                        {{ uri: 'urn:ietf:params:rtp-hdrext:sdes:mid', preferredId: 9 }}
                    ]
                }};
            }}

            if (window.RTCRtpSender && !RTCRtpSender.getCapabilities) {{
                RTCRtpSender.getCapabilities = function(kind) {{ return makeRtpCaps(kind); }};
            }}
            if (window.RTCRtpReceiver && !RTCRtpReceiver.getCapabilities) {{
                RTCRtpReceiver.getCapabilities = function(kind) {{ return makeRtpCaps(kind); }};
            }}

            if (window.screen && !window.screen.orientation) {{
                window.screen.orientation = {{
                    type: 'landscape-primary',
                    angle: 0,
                    lock: function() {{ return Promise.resolve(); }},
                    unlock: function() {{}}
                }};
            }}
        }} catch (e) {{}}

        // ================================================================
        // 6. ANTI-FINGERPRINTING FARBLING
        // ----------------------------------------------------------------
        // Thêm nhiễu vi mô vào Canvas, WebGL, và AudioContext để vô hiệu
        // các script fingerprint dựa trên exact-match hash.
        // Nhiễu nhỏ đến mức mắt và tai không nhận ra, nhưng đủ để hash khác.
        // ================================================================
        try {{
            // --- Canvas.toDataURL ---
            if (window.HTMLCanvasElement) {{
                var origToDataURL = HTMLCanvasElement.prototype.toDataURL;
                HTMLCanvasElement.prototype.toDataURL = function() {{
                    try {{
                        var ctx = this.getContext('2d');
                        if (ctx && this.width > 16 && this.height > 16) {{
                            var imgData = ctx.getImageData(0, 0, 2, 2);
                            imgData.data[0] = imgData.data[0] ^ 1;
                            ctx.putImageData(imgData, 0, 0);
                        }}
                    }} catch (e) {{}}
                    return origToDataURL.apply(this, arguments);
                }};
            }}

            // --- CanvasRenderingContext2D.getImageData ---
            if (window.CanvasRenderingContext2D) {{
                var origGetImageData = CanvasRenderingContext2D.prototype.getImageData;
                CanvasRenderingContext2D.prototype.getImageData = function() {{
                    var res = origGetImageData.apply(this, arguments);
                    try {{
                        if (res && res.data && res.data.length > 4) {{
                            res.data[0] = res.data[0] ^ 1;
                        }}
                    }} catch (e) {{}}
                    return res;
                }};
            }}

            // --- OffscreenCanvas (nếu WebKitGTK hỗ trợ) ---
            if (window.OffscreenCanvas) {{
                if (OffscreenCanvas.prototype.convertToBlob) {{
                    var origConvertToBlob = OffscreenCanvas.prototype.convertToBlob;
                    OffscreenCanvas.prototype.convertToBlob = function() {{
                        var self = this;
                        return origConvertToBlob.apply(self, arguments);
                    }};
                }}
            }}

            // --- WebGL UNMASKED_VENDOR / UNMASKED_RENDERER ---
            // WebKitGTK report "WebKit" — fingerprint hiếm. Real Chrome Linux
            // trên Intel thường trả "Google Inc. (Intel)" / "ANGLE (Intel, ...)".
            var GL_VENDOR = 0x1F00;
            var GL_RENDERER = 0x1F01;
            var UNMASKED_VENDOR = 0x9245;
            var UNMASKED_RENDERER = 0x9246;

            var FAKE_VENDOR_STRING = 'Google Inc. (Intel)';
            var FAKE_RENDERER_STRING = 'ANGLE (Intel, Mesa Intel(R) UHD Graphics 620 (KBL GT2), OpenGL 4.6 (Core Profile) Mesa 24.2.0)';

            function patchWebGL(proto) {{
                if (!proto || !proto.getParameter) return;
                var origGetParameter = proto.getParameter;
                proto.getParameter = function(pname) {{
                    if (pname === UNMASKED_VENDOR || pname === GL_VENDOR) return FAKE_VENDOR_STRING;
                    if (pname === UNMASKED_RENDERER || pname === GL_RENDERER) return FAKE_RENDERER_STRING;
                    return origGetParameter.apply(this, arguments);
                }};

                if (proto.getExtension) {{
                    var origGetExtension = proto.getExtension;
                    proto.getExtension = function(name) {{
                        var ext = origGetExtension.apply(this, arguments);
                        if (ext && name === 'WEBGL_debug_renderer_info') {{
                            try {{
                                Object.defineProperty(ext, 'UNMASKED_VENDOR_WEBGL', {{
                                    get: function() {{ return UNMASKED_VENDOR; }},
                                    configurable: true
                                }});
                                Object.defineProperty(ext, 'UNMASKED_RENDERER_WEBGL', {{
                                    get: function() {{ return UNMASKED_RENDERER; }},
                                    configurable: true
                                }});
                            }} catch (e) {{}}
                        }}
                        return ext;
                    }};
                }}
            }}

            if (window.WebGLRenderingContext) {{
                patchWebGL(WebGLRenderingContext.prototype);
            }}
            if (window.WebGL2RenderingContext) {{
                patchWebGL(WebGL2RenderingContext.prototype);
            }}

            // --- AudioContext farbling (đúng target) ---
            // Fingerprinting dùng AnalyserNode.getFloatFrequencyData /
            // getByteFrequencyData để hash output của oscillator tĩnh.
            // Add noise ±0.0001 để phá hash, tai người không nghe thấy.
            if (window.AnalyserNode) {{
                var aaMethods = [
                    'getFloatFrequencyData',
                    'getByteFrequencyData',
                    'getFloatTimeDomainData',
                    'getByteTimeDomainData'
                ];
                for (var am = 0; am < aaMethods.length; am++) {{
                    (function(methodName) {{
                        var orig = AnalyserNode.prototype[methodName];
                        if (!orig) return;
                        AnalyserNode.prototype[methodName] = function(array) {{
                            orig.call(this, array);
                            try {{
                                if (array && array.length > 0) {{
                                    for (var i = 0; i < array.length; i++) {{
                                        array[i] = array[i] + (Math.random() - 0.5) * 0.0002;
                                    }}
                                }}
                            }} catch (e) {{}}
                        }};
                    }})(aaMethods[am]);
                }}
            }}

            // --- AudioBuffer.getChannelData (giữ lại, defense in depth) ---
            if (window.AudioBuffer) {{
                var origGetChannelData = AudioBuffer.prototype.getChannelData;
                AudioBuffer.prototype.getChannelData = function() {{
                    var data = origGetChannelData.apply(this, arguments);
                    try {{
                        if (data && data.length > 0) {{
                            data[0] = data[0] + 0.00000001;
                        }}
                    }} catch (e) {{}}
                    return data;
                }};
            }}
        }} catch (e) {{}}

        // ================================================================
        // 7. PERMISSIONS API FALLBACK
        // ----------------------------------------------------------------
        // WebKitGTK đôi khi reject `navigator.permissions.query` với một số
        // permission không nhận diện được. Fallback về 'prompt' để site
        // không crash. KHÔNG spoof các permission đã được grant/denied.
        // ================================================================
        try {{
            if (navigator.permissions && navigator.permissions.query) {{
                var origQuery = navigator.permissions.query.bind(navigator.permissions);
                navigator.permissions.query = function(param) {{
                    return origQuery(param).catch(function() {{
                        return Promise.resolve({{ state: 'prompt', onchange: null }});
                    }});
                }};
            }}
        }} catch (e) {{}}
    }})();
    "#,
        major = SPOOFED_CHROME_MAJOR,
        full = SPOOFED_CHROME_FULL,
    )
}

pub fn get_autofill_script() -> &'static str {
    r#"
    (function() {
        'use strict';
        if (window.__VIBIRD_AUTOFILL) return;

        function setNativeValue(el, value) {
            if (!el) return false;
            try {
                var proto = Object.getPrototypeOf(el);
                var desc = Object.getOwnPropertyDescriptor(proto, 'value');
                if (desc && desc.set) {
                    desc.set.call(el, value);
                } else {
                    el.value = value;
                }
                el.dispatchEvent(new Event('input', { bubbles: true }));
                el.dispatchEvent(new Event('change', { bubbles: true }));
                return true;
            } catch (e) {
                return false;
            }
        }

        window.__VIBIRD_AUTOFILL = function(user, pass) {
            var pw = document.querySelector('input[type=password]:not([disabled]):not([readonly])');
            if (!pw) return false;
            setNativeValue(pw, pass);
            var form = pw.form || pw.closest('form') || document;
            var candidates = form.querySelectorAll(
                'input[type=text]:not([disabled]), input[type=email]:not([disabled]), input[name*=user i], input[name*=login i], input[name*=email i], input[autocomplete=username]'
            );
            for (var i = 0; i < candidates.length; i++) {
                var el = candidates[i];
                if (el.offsetParent !== null || el.getClientRects().length > 0) {
                    setNativeValue(el, user);
                    break;
                }
            }
            return true;
        };
    })();
    "#
}
