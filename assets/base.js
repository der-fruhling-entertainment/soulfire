function getBrowserType() {
    const test = regexp => {
        return regexp.test(navigator.userAgent);
    };

    console.log(navigator.userAgent);

    if (test(/opr\//i) || !!window.opr) {
        return 'Opera';
    } else if (test(/edg/i)) {
        return 'Microsoft Edge';
    } else if (test(/chrome|chromium|crios/i)) {
        return 'Google Chrome';
    } else if (test(/firefox|fxios/i)) {
        return 'Firefox';
    } else if (test(/safari/i)) {
        return 'Safari';
    } else if (test(/trident/i)) {
        return 'Internet Explorer';
    } else if (test(/ucbrowser/i)) {
        return 'UC Browser';
    } else if (test(/samsungbrowser/i)) {
        return 'Samsung Browser';
    } else {
        return null;
    }
}

const type = getBrowserType();

// discord requires a certain user agent
Object.defineProperty(navigator, 'userAgent', {
    get: function () { return 'DiscordBot (https://github.com/der_fruhling/, 0.1.0)'; }
});