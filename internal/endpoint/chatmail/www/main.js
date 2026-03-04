/* ── Navbar toggle ── */

function toggleNav() {
    var menu = document.getElementById('nav-menu');
    if (menu) menu.classList.toggle('navbar__menu--open');
}

/* ── Toast notification ── */

let toastEl = null;
let toastTimer = null;

function showToast(message) {
    if (!toastEl) {
        toastEl = document.createElement('div');
        toastEl.className = 'toast';
        document.body.appendChild(toastEl);
    }
    toastEl.textContent = message;
    toastEl.classList.add('toast--visible');

    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () {
        toastEl.classList.remove('toast--visible');
    }, 2000);
}

/* ── Clipboard ── */

function copyToClipboard(text) {
    if (navigator.clipboard && window.isSecureContext) {
        navigator.clipboard.writeText(text).then(function () {
            showToast("\u06a9\u067e\u06cc \u0634\u062f!");
        }).catch(function () {
            fallbackCopyTextToClipboard(text);
        });
    } else {
        fallbackCopyTextToClipboard(text);
    }
}

function fallbackCopyTextToClipboard(text) {
    var textArea = document.createElement("textarea");
    textArea.value = text;
    textArea.style.position = "fixed";
    textArea.style.left = "-9999px";
    textArea.style.top = "0";
    document.body.appendChild(textArea);
    textArea.focus();
    textArea.select();
    try {
        var successful = document.execCommand('copy');
        if (successful) showToast("\u06a9\u067e\u06cc \u0634\u062f!");
    } catch (err) {
        console.error('Fallback copy failed', err);
    }
    document.body.removeChild(textArea);
}

/* ── Email formatter ── */

function formatEmail(username, domain) {
    if (/^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$/.test(domain)) {
        return username + '@[' + domain + ']';
    }
    return username + '@' + domain;
}
