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

/* Theme bootstrap. Runs at parse time, before the body paints, so a page whose
 * stylesheet declares --default-mode does not flash the other mode first.
 * main.css is linked ahead of this script on every page that loads it.
 *
 * Four pages - docker_docs.html, relay_docs.html and their fa counterparts -
 * do not load main.js at all and carry an inline copy of this block instead. */
(function () {
  var stored = null;
  try { stored = localStorage.getItem('madmail-theme'); } catch (e) {}
  var decl = '';
  try { decl = (getComputedStyle(document.documentElement)
                 .getPropertyValue('--default-mode') || '').trim(); } catch (e) {}
  var dark = stored ? stored === 'dark'
           : decl   ? decl === 'dark'
                    : window.matchMedia('(prefers-color-scheme: dark)').matches;
  document.documentElement.classList.toggle('dark', dark);
  document.documentElement.style.colorScheme = dark ? 'dark' : 'light';
  window.toggleTheme = function () {
    var d = document.documentElement.classList.toggle('dark');
    document.documentElement.style.colorScheme = d ? 'dark' : 'light';
    try { localStorage.setItem('madmail-theme', d ? 'dark' : 'light'); } catch (e) {}
  };
})();

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
    const fb = (fallback || '127.0.0.1').replace(/^\[|\]$/g, '');
    const fromPage = (window.location.hostname || '').replace(/^\[|\]$/g, '');
    if (!fromPage || fromPage === 'localhost' || fromPage === '127.0.0.1') {
        return fb;
    }
    return fromPage;
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
