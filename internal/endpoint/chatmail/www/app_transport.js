/**
 * app_transport.js — Transport layer for Delta Chat Web
 *
 * Handles all network communication: REST API, WebSocket, and Realtime Channels.
 * Inspired by chatmail-core's peer_channels.rs which uses Iroh gossip for
 * P2P realtime communication between WebXDC instances.
 *
 * Since Iroh is not available in the browser, we emulate realtime channels
 * over the existing WebSocket+IMAP infrastructure with a pub/sub overlay.
 *
 * The app component calls:
 *   AppTransport.init(config)
 *   AppTransport.api(method, path, body)
 *   AppTransport.connectWebSocket(opts)
 *   AppTransport.disconnectWebSocket()
 *   AppTransport.joinRealtimeChannel(channelId)  -> RealtimeChannel
 */

const AppTransport = (() => {
  // ---- Internal state ----
  let _config = {
    serverUrl: '',
    email: '',
    password: '',
  };

  let _ws = null;
  let _wsReconnectTimer = null;
  let _wsPingInterval = null;
  let _wsListeners = [];
  let _wsConnected = false;
  let _wsReconnectDelay = 3000; // start at 3s, exponential backoff
  let _wantConnection = false; // true when we intentionally want to stay connected

  // Realtime channels (emulated over WebSocket + IMAP, inspired by peer_channels.rs)
  // Map of channelId -> RealtimeChannel
  const _channels = new Map();

  // Sequence numbers per channel (same concept as peer_channels.rs sequence_numbers)
  const _sequenceNumbers = new Map();

  // ---- Initialization ----

  /**
   * Initialize the transport layer with server URL and credentials.
   * @param {Object} config - { serverUrl, email, password }
   */
  function init(config) {
    _config.serverUrl = (config.serverUrl || '').replace(/\/$/, '');
    _config.email = config.email || '';
    _config.password = config.password || '';
  }

  /**
   * Update credentials (e.g. after re-registration).
   * @param {string} email
   * @param {string} password
   */
  function setCredentials(email, password) {
    _config.email = email;
    _config.password = password;
  }

  // ---- REST API ----

  /**
   * Make an authenticated API call.
   * @param {string} method - HTTP method
   * @param {string} path - URL path (e.g. '/webimap/mailboxes')
   * @param {Object} [body] - JSON body
   * @returns {Promise<any>}
   */
  async function api(method, path, body) {
    const url = _config.serverUrl + path;
    AppLog.debug('Transport', `API Request: ${method} ${path}`, { email: _config.email });
    const opts = {
      method,
      headers: {
        'X-Email': _config.email,
        'X-Password': _config.password,
        'Content-Type': 'application/json',
      },
    };
    if (body) opts.body = JSON.stringify(body);
    const res = await fetch(url, opts);
    const data = await res.json();
    if (!res.ok) {
        AppLog.error('Transport', `API Error: ${method} ${path} -> ${res.status}`, data);
        throw new Error(data.error || `HTTP ${res.status}`);
    }
    AppLog.debug('Transport', `API Success: ${method} ${path}`);
    return data;
  }

  /**
   * Send e-mail via WebSMTP REST endpoint.
   * @param {string} from - Sender email
   * @param {string[]} to - Recipient emails
   * @param {string} rawBody - Full RFC5322 raw message (headers + body)
   * @returns {Promise<any>}
   */
  async function sendMail(from, to, rawBody) {
    AppLog.info('Transport', `Sending mail to ${to.join(', ')}`, { size: rawBody.length });
    return api('POST', '/webimap/send', { from, to, body: rawBody });
  }

  // ---- WebSocket ----

  /**
   * Connect WebSocket for real-time IMAP push notifications.
   * @param {Object} opts - { mailbox, sinceUID, onMessage, onStatusChange }
   * @param {boolean} [_isReconnect=false] - Internal flag, do not set manually
   * @returns {void}
   */
  function connectWebSocket(opts = {}, _isReconnect = false) {
    const mailbox = opts.mailbox || 'INBOX';
    const sinceUID = opts.sinceUID || 0;
    const onMessage = opts.onMessage || (() => {});
    const onStatusChange = opts.onStatusChange || (() => {});

    // Only reset backoff on intentional (non-reconnect) calls
    if (!_isReconnect) {
      _wsReconnectDelay = 3000;
    }

    _wantConnection = true;
    AppLog.info('Transport', 'Starting WebSocket connection', { mailbox, sinceUID, isReconnect: _isReconnect });


    // Clear any pending reconnect
    if (_wsReconnectTimer) {
      clearTimeout(_wsReconnectTimer);
      _wsReconnectTimer = null;
    }

    // Clear any existing ping interval
    if (_wsPingInterval) {
      clearInterval(_wsPingInterval);
      _wsPingInterval = null;
    }

    if (_ws) {
      try { _ws.close(); } catch (e) { /* ignore */ }
    }

    const wsProto = _config.serverUrl.startsWith('https') ? 'wss' : 'ws';
    const host = _config.serverUrl.replace(/^https?:\/\//, '');
    const wsUrl = `${wsProto}://${host}/webimap/ws?email=${encodeURIComponent(_config.email)}&password=${encodeURIComponent(_config.password)}&mailbox=${encodeURIComponent(mailbox)}&since_uid=${sinceUID}`;

    _ws = new WebSocket(wsUrl);
    _wsConnected = false;

    _ws.onopen = () => {
      AppLog.info('Transport', 'WebSocket connected');
      _wsConnected = true;
      _wsReconnectDelay = 3000; // reset backoff on successful connect
      AppLog.success('Transport', 'WebSocket connected');
      onStatusChange('connected');

      // Start ping/keepalive every 25s to prevent idle timeouts
      _wsPingInterval = setInterval(() => {
        if (_ws && _ws.readyState === WebSocket.OPEN) {
          try {
            _ws.send(JSON.stringify({ type: 'ping' }));
          } catch (e) {
            // Send failed, connection is broken — close to trigger reconnect
            AppLog.warn('Transport', 'Ping failed, closing WS');
            _ws.close();
          }
        }
      }, 25000);
    };

    _ws.onmessage = async (event) => {
      try {
        const msg = JSON.parse(event.data);
        AppLog.debug('Transport', 'Received message', msg);

        // Dispatch to all registered message listeners
        onMessage(msg);

        // Also dispatch to realtime channel listeners if applicable
        _dispatchToChannels(msg);

      } catch (e) {
        AppLog.error('Transport', 'Failed to process WS message: ' + e.message, e.stack);
      }
    };

    _ws.onclose = () => {
      AppLog.info('Transport', 'WebSocket closed');
      _wsConnected = false;
      AppLog.warn('Transport', 'WebSocket closed');
      onStatusChange('disconnected');

      // Stop ping
      if (_wsPingInterval) {
        clearInterval(_wsPingInterval);
        _wsPingInterval = null;
      }

      // Auto-reconnect with exponential backoff (only if we want to stay connected)
      if (_wantConnection && !_wsReconnectTimer) {
        const delay = _wsReconnectDelay;
        _wsReconnectDelay = Math.min(_wsReconnectDelay * 2, 30000); // double each time, max 30s
        _wsReconnectTimer = setTimeout(() => {
          _wsReconnectTimer = null;
          if (_wantConnection) {
            AppLog.info('Transport', `Reconnecting WebSocket (backoff: ${delay}ms)...`);
            AppLog.info('Transport', `Reconnecting (backoff: ${delay}ms)`);
            connectWebSocket(opts, true); // true = this is a reconnect, don't reset backoff
          }
        }, delay);
      }
    };

    _ws.onerror = (e) => {
      AppLog.error('Transport', 'WebSocket error', { error: e });
      onStatusChange('error');
    };
  }

  /**
   * Disconnect the WebSocket and stop reconnection.
   */
  function disconnectWebSocket() {
    _wantConnection = false;
    if (_wsReconnectTimer) {
      clearTimeout(_wsReconnectTimer);
      _wsReconnectTimer = null;
    }
    if (_ws) {
      _ws.close();
      _ws = null;
    }
    _wsConnected = false;
  }

  /**
   * Check if WebSocket is currently connected.
   * @returns {boolean}
   */
  function isWebSocketConnected() {
    return _wsConnected && _ws && _ws.readyState === WebSocket.OPEN;
  }

  // ---- Realtime Channels ----
  //
  // Inspired by chatmail-core's peer_channels.rs:
  //
  // In native Delta Chat, peer channels use Iroh gossip over QUIC for
  // low-latency P2P communication. Each WebXDC gets a TopicId, peers
  // advertise their NodeAddr, and then join a gossip swarm.
  //
  // In the browser, we emulate this with a thin overlay on top of
  // WebSocket + IMAP. Each "channel" has a topic string and provides
  // the same API as the Webxdc realtime spec:
  //
  //   - setListener(callback)   - receive Uint8Array data from peers
  //   - send(data)              - send Uint8Array data to peers
  //   - leave()                 - leave the channel
  //
  // Data is transported as hidden IMAP messages with a special
  // X-Realtime-Channel header. The WebSocket delivers them in real-time
  // and the channel layer filters + dispatches.

  /**
   * A realtime channel — mirrors the Webxdc joinRealtimeChannel() API.
   *
   * @see https://webxdc.org/docs/spec/joinRealtimeChannel.html
   * @see chatmail-core/src/peer_channels.rs
   */
  class RealtimeChannel {
    constructor(channelId) {
      this.channelId = channelId;
      this._listener = null;
      this._active = true;
      this._seqNum = 0;
    }

    /**
     * Set a listener for incoming realtime data.
     * Mirrors realtimeChannel.setListener((data) => {})
     * @param {function(Uint8Array)} callback
     */
    setListener(callback) {
      this._listener = callback;
    }

    /**
     * Send data to all peers on this channel.
     * Mirrors realtimeChannel.send(data)
     * Data is sent as a hidden message via IMAP with X-Realtime-Channel header.
     * Max size: 128000 bytes (matching Iroh gossip max_message_size).
     *
     * @param {Uint8Array|string} data - Data to send (max 128KB)
     */
    async send(data) {
      if (!this._active) {
        throw new Error('Channel is no longer active. Call joinRealtimeChannel() again.');
      }

      // Convert string to Uint8Array if needed
      let payload;
      if (typeof data === 'string') {
        payload = new TextEncoder().encode(data);
      } else if (data instanceof Uint8Array) {
        payload = data;
      } else {
        throw new Error('Data must be Uint8Array or string');
      }

      if (payload.length > 128000) {
        throw new Error(`Data exceeds max size: ${payload.length} > 128000 bytes`);
      }

      this._seqNum++;

      // Encode as base64 for transport over IMAP
      const b64Data = _uint8ArrayToBase64(payload);

      // Send as a hidden IMAP message with channel metadata
      const channelMsg = {
        channelId: this.channelId,
        seqNum: this._seqNum,
        sender: _config.email,
        data: b64Data,
        timestamp: Date.now(),
      };

      try {
        // Use the REST API to store a channel message
        // The message is a special hidden format that won't appear
        // in normal chat views
        await _sendChannelMessage(channelMsg);
      } catch (e) {
        AppLog.error('Transport', 'Failed to send channel data: ' + e.message, e.stack);
      }
    }

    /**
     * Leave the realtime channel.
     * Mirrors realtimeChannel.leave()
     * After this, the channel is invalid. Call joinRealtimeChannel() again to rejoin.
     */
    leave() {
      this._active = false;
      this._listener = null;
      _channels.delete(this.channelId);
      AppLog.info('Transport', `Left realtime channel: ${this.channelId}`);
    }

    /**
     * Internal: dispatch received data to the listener.
     * @param {Uint8Array} data
     */
    _dispatch(data) {
      if (this._active && this._listener) {
        this._listener(data);
      }
    }
  }

  /**
   * Join a realtime channel — mirrors window.webxdc.joinRealtimeChannel().
   *
   * Creates or retrieves a RealtimeChannel for the given channelId.
   * If no channelId is provided, a random one is generated (like create_random_topic()
   * in peer_channels.rs).
   *
   * @param {string} [channelId] - Optional channel/topic identifier
   * @returns {RealtimeChannel}
   */
  function joinRealtimeChannel(channelId) {
    if (!channelId) {
      channelId = _generateTopicId();
    }

    // Like iroh_channels in peer_channels.rs — check if already joined
    if (_channels.has(channelId)) {
      return _channels.get(channelId);
    }

    const channel = new RealtimeChannel(channelId);
    _channels.set(channelId, channel);

    AppLog.info('Transport', `Joined realtime channel: ${channelId}`);
    return channel;
  }

  /**
   * Get an existing channel by ID.
   * @param {string} channelId
   * @returns {RealtimeChannel|null}
   */
  function getChannel(channelId) {
    return _channels.get(channelId) || null;
  }

  /**
   * Leave all realtime channels.
   */
  function leaveAllChannels() {
    for (const [id, channel] of _channels) {
      channel.leave();
    }
    _channels.clear();
  }

  // ---- Internal helpers ----

  /**
   * Generate a random topic ID (32 bytes, base32-encoded).
   * Mirrors create_random_topic() in peer_channels.rs.
   * @returns {string}
   */
  function _generateTopicId() {
    const bytes = new Uint8Array(32);
    crypto.getRandomValues(bytes);
    // Simple hex encoding for browser (no base32 lib needed)
    return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
  }

  /**
   * Convert Uint8Array to base64 string.
   * @param {Uint8Array} bytes
   * @returns {string}
   */
  function _uint8ArrayToBase64(bytes) {
    let binary = '';
    for (let i = 0; i < bytes.length; i++) {
      binary += String.fromCharCode(bytes[i]);
    }
    return btoa(binary);
  }

  /**
   * Convert base64 string to Uint8Array.
   * @param {string} b64
   * @returns {Uint8Array}
   */
  function _base64ToUint8Array(b64) {
    const binary = atob(b64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }

  /**
   * Send a realtime channel message via IMAP.
   * The message is a hidden system message with X-Realtime-Channel header.
   * @param {Object} channelMsg - { channelId, seqNum, sender, data, timestamp }
   */
  async function _sendChannelMessage(channelMsg) {
    const msgId = `<rt-${_generateShortId()}@${_config.email.split('@')[1]}>`;
    const now = new Date().toUTCString();

    // Build a minimal hidden message for channel transport
    const rawEmail = [
      `From: <${_config.email}>`,
      `Date: ${now}`,
      `Message-ID: ${msgId}`,
      `Subject: [...]`,
      `Chat-Version: 1.0`,
      `X-Realtime-Channel: ${channelMsg.channelId}`,
      `X-Realtime-SeqNum: ${channelMsg.seqNum}`,
      `Content-Type: text/plain; charset=utf-8`,
      `MIME-Version: 1.0`,
      '',
      channelMsg.data,
    ];

    // Send to self — the message will arrive via WebSocket
    // and be dispatched to channel listeners
    await sendMail(_config.email, [_config.email], rawEmail.join('\r\n'));
  }

  /**
   * Generate a short random ID.
   * @returns {string}
   */
  function _generateShortId() {
    const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    let result = '';
    for (let i = 0; i < 16; i++) {
      result += chars[Math.floor(Math.random() * chars.length)];
    }
    return result;
  }

  /**
   * Dispatch incoming WebSocket messages to realtime channels.
   * Checks for the X-Realtime-Channel header in the message body.
   * @param {Object} msg - The raw IMAP message from WebSocket
   */
  function _dispatchToChannels(msg) {
    if (!msg.body || _channels.size === 0) return;

    // Quick check for realtime channel header
    const headerMatch = msg.body.match(/^X-Realtime-Channel:\s*(.+)$/mi);
    if (!headerMatch) return;

    const channelId = headerMatch[1].trim();
    const channel = _channels.get(channelId);
    if (!channel) return;

    // Extract the base64 data from the body
    const bodyParts = msg.body.split(/\r?\n\r?\n/);
    if (bodyParts.length < 2) return;

    const b64Data = bodyParts[bodyParts.length - 1].trim();
    if (!b64Data) return;

    try {
      const data = _base64ToUint8Array(b64Data);

      // Don't dispatch our own messages back (like iroh does with public_key check)
      const fromMatch = msg.body.match(/^From:\s*<?([^>\s]+)>?$/mi);
      if (fromMatch && fromMatch[1].toLowerCase() === _config.email.toLowerCase()) {
        return;
      }

      channel._dispatch(data);
    } catch (e) {
      AppLog.error('Transport', 'Failed to decode channel data: ' + e.message, e.stack);
    }
  }

  /**
   * Check if an IMAP message is a realtime channel message (hidden).
   * This helps the app layer skip these messages when rendering chat.
   * @param {string} body - Raw message body
   * @returns {boolean}
   */
  function isChannelMessage(body) {
    if (!body) return false;
    return /^X-Realtime-Channel:/mi.test(body);
  }

  // ---- Public API ----
  return {
    init,
    setCredentials,
    api,
    sendMail,
    connectWebSocket,
    disconnectWebSocket,
    isWebSocketConnected,
    joinRealtimeChannel,
    getChannel,
    leaveAllChannels,
    isChannelMessage,
    RealtimeChannel,
  };
})();
