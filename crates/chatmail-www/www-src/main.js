/*
 * Copyright (C) 2026 themadorg
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

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
            showToast(t("toast_copied"));
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
        if (successful) showToast(t("toast_copied"));
    } catch (err) {
        console.error('Fallback copy failed', err);
    }
    document.body.removeChild(textArea);
}

/* ── Email formatter ── */

function formatEmail(username, domain) {
    const bare = String(domain).trim().replace(/^\[|\]$/g, '');
    if (/^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$/.test(bare)) {
        return username + '@[' + bare + ']';
    }
    return username + '@' + bare;
}

/** Bare hostname for dclogin `ih` / `sh` (no brackets). */
function connectHostForDclogin(fallback) {
    const fromPage = window.location.hostname;
    if (fromPage) {
        return fromPage.replace(/^\[|\]$/g, '');
    }
    return (fallback || '127.0.0.1').replace(/^\[|\]$/g, '');
}

/** Render a dclogin / invite QR into an <img> (client-side, no /qr backend). */
function setQrCodeImage(imgEl, text, cellSize) {
    if (!imgEl || !text || typeof qrcode !== 'function') {
        return;
    }
    try {
        var qr = qrcode(0, 'M');
        qr.addData(text);
        qr.make();
        imgEl.src = qr.createDataURL(cellSize || 4, 2);
        imgEl.alt = 'QR Code';
    } catch (err) {
        console.error('QR generation failed', err);
    }
}
