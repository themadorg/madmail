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

/**
 * app_log.js - Debug Logging Service for Madmail Web
 * Maintain a history of events, errors, and incoming messages.
 */
const AppLog = (() => {
  const MAX_LOGS = 1000;
  let _logs = [];
  let _onUpdate = null;

  function _add(level, category, message, data = null) {
    const entry = {
      timestamp: new Date().toISOString(),
      time: new Date().toLocaleTimeString(),
      level,      // 'info', 'success', 'warn', 'error', 'debug'
      category,   // 'Transport', 'SecureJoin', 'DB', 'App', 'Message'
      message,
      data: data ? _sanitize(data) : null
    };

    _logs.unshift(entry);
    if (_logs.length > MAX_LOGS) _logs.pop();

    if (_onUpdate) _onUpdate(_logs);
  }

  function _sanitize(data) {
    // Deep clone and truncate big fields
    try {
      const clean = JSON.parse(JSON.stringify(data));
      const truncate = (obj) => {
        for (const key in obj) {
          if (typeof obj[key] === 'string' && obj[key].length > 200) {
            obj[key] = obj[key].substring(0, 100) + '... (truncated)';
          } else if (typeof obj[key] === 'object' && obj[key] !== null) {
            truncate(obj[key]);
          }
        }
      };
      truncate(clean);
      return clean;
    } catch (e) {
      return '[Complex Data]';
    }
  }

  return {
    info: (cat, msg, data) => _add('info', cat, msg, data),
    success: (cat, msg, data) => _add('success', cat, msg, data),
    warn: (cat, msg, data) => _add('warn', cat, msg, data),
    error: (cat, msg, data) => _add('error', cat, msg, data),
    debug: (cat, msg, data) => _add('debug', cat, msg, data),

    getLogs: () => _logs,
    clear: () => { _logs = []; if(_onUpdate) _onUpdate([]); },
    onUpdate: (cb) => { _onUpdate = cb; },

    getCopyableText: () => {
      return _logs.map(l => 
        `[${l.timestamp}] [${l.level.toUpperCase()}] [${l.category}] ${l.message} ${l.data ? JSON.stringify(l.data) : ''}`
      ).join('\n');
    }
  };
})();

if (typeof window !== 'undefined') {
  window.AppLog = AppLog;
}
