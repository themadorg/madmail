function app() {
  return {
    // State machine: init -> setup -> ready
    state: 'init',
    initStatus: 'Connecting...',
    error: '',
    connecting: false,

    // Config
    serverUrl: '',

    // Credentials (auto-generated)
    credentials: { email: '', password: '' },
    displayName: '',
    profileImageB64: '', // base64 data URL for Chat-User-Avatar

    // PGP
    pgpPrivateKey: null,
    pgpPublicKey: null,
    publicKeyArmored: '',
    privateKeyArmored: '',
    pgpFingerprint: '',
    autocryptKeydata: '',

    // Contacts & chat
    contacts: [],
    selectedContact: null,
    newContactAddr: '',
    composeText: '',
    composeImage: null, // data URL of image to send
    sending: false,
    polling: false,
    ws: null,
    lastUID: 0,
    processedUIDs: new Set(), // track ALL processed UIDs (including handshake)
    allMessages: [], // visible messages from IMAP

    // UI
    showInfo: false,
    contextMenu: { show: false, x: 0, y: 0, msg: null },
    longPressTimer: null,
    emojiPickerOpen: false,
    emojiPickerMsg: null,

    // SecureJoin
    securejoinUri: '',
    qrDataUrl: '',

    async init() {
      this.serverUrl = window.location.origin;
      await this.openDB();
      const restored = await this.restoreState();
      if (restored) {
        // Restored from IndexedDB — reconnect WebSocket
        this.state = 'ready';
        this.toast('success', `Restored session: ${this.credentials.email}`);
        this.connectWebSocket();
      } else {
        // Fresh start — register
        await this.connect();
      }
    },

    // ---- IndexedDB (via AppDB module in app_db.js) ----
    async openDB() {
      await AppDB.open();
    },

    async saveState() {
      await AppDB.save(this);
    },

    async restoreState() {
      const data = await AppDB.load();
      if (!data) return false;
      try {
        this.credentials = data.credentials;
        this.displayName = data.displayName || '';
        this.profileImageB64 = data.profileImageB64 || '';
        this.publicKeyArmored = data.publicKeyArmored;
        this.privateKeyArmored = data.privateKeyArmored;
        this.pgpFingerprint = data.pgpFingerprint;
        this.autocryptKeydata = data.autocryptKeydata || this.extractAutocryptKeydata(data.publicKeyArmored);
        this.pgpPrivateKey = await openpgp.readPrivateKey({ armoredKey: data.privateKeyArmored });
        this.pgpPublicKey = await openpgp.readKey({ armoredKey: data.publicKeyArmored });
        this.contacts = (data.contacts || []).map(c => ({ ...c, unread: 0 }));
        this.allMessages = data.allMessages || [];
        this.lastUID = data.lastUID || 0;
        this.securejoinUri = data.securejoinUri || '';
        // Rebuild processedUIDs from loaded messages
        for (const m of this.allMessages) this.processedUIDs.add(m.uid);
        // Regenerate QR if needed
        if (this.securejoinUri) {
          this.qrDataUrl = await this.generateQRDataUrl(this.securejoinUri).catch(() => '');
        }
        // Verify IMAP access still works
        await this.api('GET', '/webimap/mailboxes');
        return true;
      } catch (e) {
        console.warn('Restore failed, will re-register:', e.message);
        return false;
      }
    },

    async clearState() {
      await AppDB.clear();
    },

    // ---- API helpers ----
    async api(method, path, body) {
      const url = this.serverUrl.replace(/\/$/, '') + path;
      const opts = {
        method,
        headers: {
          'X-Email': this.credentials.email,
          'X-Password': this.credentials.password,
          'Content-Type': 'application/json',
        },
      };
      if (body) opts.body = JSON.stringify(body);
      const res = await fetch(url, opts);
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`);
      return data;
    },

    // ---- Connection flow ----
    async connect() {
      this.connecting = true;
      this.error = '';
      try {
        // Step 1: Auto-register
        this.state = 'init';
        this.initStatus = 'Registering account...';

        const regRes = await fetch(this.serverUrl.replace(/\/$/, '') + '/new', { method: 'POST' });
        if (!regRes.ok) {
          const txt = await regRes.text();
          throw new Error('Registration failed: ' + txt);
        }
        const creds = await regRes.json();
        this.credentials = creds;
        this.initStatus = `Registered as ${creds.email}`;

        // Step 2: Generate PGP keys
        this.initStatus = 'Generating PGP keys (Ed25519/Curve25519)...';
        await this.generatePGPKeys();

        // Step 3: Verify IMAP access
        this.initStatus = 'Verifying IMAP access...';
        await this.api('GET', '/webimap/mailboxes');

        // Step 4: Generate SecureJoin QR
        this.initStatus = 'Setting up SecureJoin...';
        await this.generateSecureJoinQR();

        // Save to IndexedDB
        await this.saveState();

        // Ready!
        this.state = 'ready';
        this.toast('success', `Connected as ${creds.email}`);

        // Start WebSocket for real-time messages
        this.connectWebSocket();

      } catch (e) {
        this.error = e.message;
        this.state = 'setup';
      } finally {
        this.connecting = false;
      }
    },

    // ---- PGP ----
    async generatePGPKeys() {
      const { privateKey, publicKey } = await openpgp.generateKey({
        type: 'ecc',
        curve: 'curve25519',
        userIDs: [{ name: this.displayName || undefined, email: this.credentials.email }],
        passphrase: '',
        format: 'armored',
      });

      this.pgpPrivateKey = await openpgp.readPrivateKey({ armoredKey: privateKey });
      this.pgpPublicKey = await openpgp.readKey({ armoredKey: publicKey });
      this.publicKeyArmored = publicKey;
      this.privateKeyArmored = privateKey;
      this.pgpFingerprint = this.pgpPublicKey.getFingerprint().toUpperCase();

      // Extract raw base64 keydata for Autocrypt header
      this.autocryptKeydata = this.extractAutocryptKeydata(publicKey);
    },

    // Build From header with optional display name
    buildFromHeader() {
      return this.displayName
        ? `From: "${this.displayName}" <${this.credentials.email}>`
        : `From: <${this.credentials.email}>`;
    },

    // Handle avatar file upload
    async handleAvatarUpload(event) {
      const file = event.target.files[0];
      if (!file) return;
      // Resize to max 192x192 (Delta Chat convention)
      const canvas = document.createElement('canvas');
      const img = new Image();
      img.src = URL.createObjectURL(file);
      await new Promise(r => img.onload = r);
      const size = Math.min(img.width, img.height, 192);
      canvas.width = size;
      canvas.height = size;
      const ctx = canvas.getContext('2d');
      // Center crop
      const sx = (img.width - size) / 2;
      const sy = (img.height - size) / 2;
      ctx.drawImage(img, sx, sy, size, size, 0, 0, size, size);
      this.profileImageB64 = canvas.toDataURL('image/jpeg', 0.85);
      URL.revokeObjectURL(img.src);
      await this.saveState();
      this.toast('success', 'Profile image updated');
    },

    // Extract raw base64 keydata from armored key (for Autocrypt header)
    extractAutocryptKeydata(armoredKey) {
      const lines = armoredKey.split(/\r?\n/);
      let inBody = false;
      const b64Lines = [];
      for (const line of lines) {
        if (line === '') {
          // Blank line marks end of armor headers, start of body
          inBody = true;
          continue;
        }
        if (!inBody) continue;
        if (line.startsWith('-----END')) break;
        if (line.startsWith('=')) continue; // CRC24 checksum line
        b64Lines.push(line.trim());
      }
      return b64Lines.join('');
    },

    // Build a properly folded Autocrypt header
    buildAutocryptHeader() {
      const keydata = this.autocryptKeydata;
      // Fold keydata into continuation lines (76 chars each)
      let folded = '';
      for (let i = 0; i < keydata.length; i += 76) {
        if (i > 0) folded += '\r\n ';
        folded += keydata.substring(i, i + 76);
      }
      return `Autocrypt: addr=${this.credentials.email}; prefer-encrypt=mutual;\r\n keydata=${folded}`;
    },

    async encryptMessage(text, recipientPubKeyArmored, opts = {}) {
      let encryptKeys = [this.pgpPublicKey]; // always encrypt to self
      if (recipientPubKeyArmored) {
        const recipientKey = await openpgp.readKey({ armoredKey: recipientPubKeyArmored });
        encryptKeys.push(recipientKey);
      }

      // Build a proper MIME message inside the encrypted payload.
      // Delta Chat expects Content-Type + protected headers inside the PGP body.
      const to = opts.to || '';
      const date = opts.date || new Date().toUTCString();
      const extraHeaders = opts.extraHeaders || '';
      const attachment = opts.attachment || null; // { data, mimeType, filename }

      let mimePayload;

      if (attachment) {
        // multipart/mixed: text + image (matches Delta Chat core build_body_file)
        const boundary = 'dc-mixed-' + Math.random().toString(36).slice(2, 10);
        const parts = [
          `Content-Type: multipart/mixed; boundary="${boundary}"; protected-headers="v1"`,
          this.buildFromHeader(),
          `To: <${to}>`,
          `Date: ${date}`,
        ];
        if (extraHeaders) parts.push(extraHeaders);
        parts.push(
          '',
          `--${boundary}`,
          'Content-Type: text/plain; charset="utf-8"',
          '',
          text || '',
          '',
          `--${boundary}`,
          `Content-Type: ${attachment.mimeType}; name="${attachment.filename}"`,
          `Content-Disposition: attachment; filename="${attachment.filename}"`,
          'Content-Transfer-Encoding: base64',
          '',
          attachment.data, // raw base64
          '',
          `--${boundary}--`,
        );
        mimePayload = parts.join('\r\n');
      } else {
        // Simple text-only message
        const innerMime = [
          `Content-Type: text/plain; charset="utf-8"; protected-headers="v1"`,
          this.buildFromHeader(),
          `To: <${to}>`,
          `Date: ${date}`,
        ];
        if (extraHeaders) innerMime.push(extraHeaders);
        innerMime.push('', text);
        mimePayload = innerMime.join('\r\n');
      }

      const encrypted = await openpgp.encrypt({
        message: await openpgp.createMessage({ text: mimePayload }),
        encryptionKeys: encryptKeys,
        signingKeys: this.pgpPrivateKey,
      });

      return encrypted; // armored PGP message
    },

    async decryptMessage(armoredMessage) {
      try {
        const message = await openpgp.readMessage({ armoredMessage });
        const { data, signatures } = await openpgp.decrypt({
          message,
          decryptionKeys: this.pgpPrivateKey,
        });
        return { text: data, encrypted: true };
      } catch (e) {
        // Not encrypted, return as-is
        return { text: armoredMessage, encrypted: false };
      }
    },

    // ---- SecureJoin QR ----
    async generateSecureJoinQR() {
      const fp = this.pgpFingerprint;
      const email = this.credentials.email;
      const inviteNumber = this.generateId();
      const auth = this.generateId();

      this.securejoinUri = `https://i.delta.chat/#${fp}&i=${inviteNumber}&s=${auth}&a=${encodeURIComponent(email)}`;

      // Generate QR using a simple canvas-based approach
      try {
        // Use a basic QR library or just display the URI
        this.qrDataUrl = await this.generateQRDataUrl(this.securejoinUri);
      } catch (e) {
        // Fallback - no QR image
        this.qrDataUrl = '';
      }
    },

    async generateQRDataUrl(text) {
      // Simple QR code generation using canvas
      // We'll use a lightweight approach
      const size = 200;
      const canvas = document.createElement('canvas');
      canvas.width = size;
      canvas.height = size;
      const ctx = canvas.getContext('2d');

      // Simple placeholder - in production, use a QR library
      ctx.fillStyle = '#fff';
      ctx.fillRect(0, 0, size, size);
      ctx.fillStyle = '#000';
      ctx.font = '10px monospace';
      ctx.textAlign = 'center';

      // Render the fingerprint as a pattern
      const lines = text.match(/.{1,30}/g) || [text];
      lines.forEach((line, i) => {
        ctx.fillText(line, size/2, 20 + i * 14);
      });

      return canvas.toDataURL();
    },

    generateId() {
      const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
      let result = '';
      for (let i = 0; i < 16; i++) {
        result += chars[Math.floor(Math.random() * chars.length)];
      }
      return result;
    },

    // ---- Contacts ----
    addContactOrScan() {
      const input = this.newContactAddr.trim();
      if (!input) return;

      // Check if it's a SecureJoin URI
      if (input.startsWith('https://i.delta.chat/#') || input.startsWith('OPENPGP4FPR:')) {
        this.handleSecureJoinScan(input);
      } else if (input.includes('@')) {
        this.addContact(input);
      } else {
        this.toast('error', 'Enter an email address or SecureJoin URI');
      }
      this.newContactAddr = '';
    },

    addContact(addr) {
      addr = addr.trim().toLowerCase();
      if (!addr || !addr.includes('@')) return;
      if (this.contacts.find(c => c.email === addr)) {
        this.toast('info', 'Contact already exists');
        return;
      }
      this.contacts.push({
        email: addr,
        pgpKey: null,
        fingerprint: null,
        verified: false,
        unread: 0,
        lastMsg: '',
      });
      this.toast('success', `Added ${addr}`);
      return this.contacts.find(c => c.email === addr);
    },

    // ---- SecureJoin protocol (Bob/Joiner side) ----
    async handleSecureJoinScan(uri) {
      try {
        // Parse: https://i.delta.chat/#FINGERPRINT&i=INVITE&s=AUTH&a=EMAIL&n=NAME
        let fragment;
        if (uri.startsWith('OPENPGP4FPR:')) {
          fragment = uri.substring('OPENPGP4FPR:'.length);
        } else {
          const hashIdx = uri.indexOf('#');
          if (hashIdx < 0) throw new Error('Invalid QR URI');
          fragment = uri.substring(hashIdx + 1);
        }

        // Split by & — first part is fingerprint
        const parts = fragment.split('&');
        const fingerprint = parts[0].toUpperCase();
        const params = {};
        for (let i = 1; i < parts.length; i++) {
          const [k, v] = parts[i].split('=');
          params[k] = decodeURIComponent(v || '');
        }

        const inviterEmail = params.a;
        const inviteNumber = params.i;
        const auth = params.s;
        const inviterName = (params.n || '').replace(/\+/g, ' ');
        const grpId = params.x;

        if (!inviterEmail || !inviteNumber) {
          throw new Error('Missing required fields in QR code');
        }

        this.toast('info', `SecureJoin: contacting ${inviterName || inviterEmail}...`);

        // Add or get contact
        let contact = this.contacts.find(c => c.email === inviterEmail.toLowerCase());
        if (!contact) {
          contact = this.addContact(inviterEmail);
        }
        contact.fingerprint = fingerprint;
        contact.securejoinAuth = auth;
        contact.securejoinInvite = inviteNumber;

        // Step 2 (Bob): Send vc-request to Alice
        // This is a SecureJoin handshake — server allows it through unencrypted
        await this.sendSecureJoinRequest(inviterEmail, inviteNumber, grpId);

        this.selectContact(contact);
        this.toast('success', `SecureJoin request sent to ${inviterEmail}`);

      } catch (e) {
        this.toast('error', 'SecureJoin failed: ' + e.message);
      }
    },

    async sendSecureJoinRequest(toEmail, inviteNumber, grpId) {
      const step = grpId ? 'vg-request' : 'vc-request';
      const msgId = `<${this.generateId()}@${this.credentials.email.split('@')[1]}>`;
      const boundary = 'securejoin-' + this.generateId();
      const now = new Date().toUTCString();

      const rawEmail = [
        this.buildFromHeader(),
        `To: <${toEmail}>`,
        `Date: ${now}`,
        `Message-ID: ${msgId}`,
        `Subject: [...]`,
        `Chat-Version: 1.0`,
        `Secure-Join: ${step}`,
        `Secure-Join-Invitenumber: ${inviteNumber}`,
      ];

      // Add Autocrypt header with proper folding
      rawEmail.push(this.buildAutocryptHeader());

      rawEmail.push(
        `Content-Type: multipart/mixed; boundary="${boundary}"`,
        `MIME-Version: 1.0`,
        '',
        `--${boundary}`,
        'Content-Type: text/plain; charset=utf-8',
        '',
        `secure-join: ${step}`,
        '',
        `--${boundary}--`,
      );

      await this.api('POST', '/webimap/send', {
        from: this.credentials.email,
        to: [toEmail],
        body: rawEmail.join('\r\n'),
      });
    },

    selectContact(contact) {
      this.selectedContact = contact;
      contact.unread = 0;
      this.contextMenu.show = false;
      this.$nextTick(() => this.scrollToBottom());
    },

    async deleteChat(contact) {
      if (!confirm(`Delete all messages with ${contact.email}?\n\nThis cannot be undone.`)) return;
      // Remove all messages for this contact
      this.allMessages = this.allMessages.filter(m => m.peerEmail !== contact.email);
      // Remove contact from list
      const idx = this.contacts.indexOf(contact);
      if (idx >= 0) this.contacts.splice(idx, 1);
      // Deselect
      if (this.selectedContact === contact) this.selectedContact = null;
      await this.saveState();
      this.toast('success', `Chat with ${contact.email} deleted`);
    },

    // ---- Context menu & Delete ----
    showContextMenu(event, msg) {
      this.contextMenu = {
        show: true,
        x: Math.min(event.clientX, window.innerWidth - 180),
        y: Math.min(event.clientY, window.innerHeight - 150),
        msg,
      };
    },

    startLongPress(event, msg) {
      this.longPressTimer = setTimeout(() => {
        const touch = event.touches[0];
        this.showContextMenu({ clientX: touch.clientX, clientY: touch.clientY }, msg);
      }, 500);
    },

    cancelLongPress() {
      if (this.longPressTimer) {
        clearTimeout(this.longPressTimer);
        this.longPressTimer = null;
      }
    },

    async deleteMessage(msg) {
      this.contextMenu.show = false;
      if (!msg) return;

      // Remove from local state
      const idx = this.allMessages.findIndex(m => m.uid === msg.uid);
      if (idx >= 0) this.allMessages.splice(idx, 1);

      // Delete from IMAP server (set \Deleted + Expunge, matching core's delete_msgs)
      // Only for real IMAP UIDs (not locally-generated sentUIDs)
      if (typeof msg.uid === 'number' && msg.uid < 1e10) {
        try {
          await this.api('DELETE', `/webimap/messages/INBOX/${msg.uid}`);
          console.log(`Deleted message UID ${msg.uid} from server`);
        } catch (e) {
          console.warn('Server delete failed (message may already be gone):', e.message);
        }
      }

      await this.saveState();
    },

    async deleteForEveryone(msg) {
      this.contextMenu.show = false;
      if (!msg || !msg.outgoing || !msg.peerEmail) return;

      // First delete locally
      await this.deleteMessage(msg);

      // Send a hidden delete-request message (matching core's delete_msgs_ex with delete_for_all=true)
      // Core uses Chat-Delete header INSIDE the encrypted body (it's a "hidden" header per mimeparser.rs:2231)
      // The receiver's handle_edit_delete() reads Chat-Delete from the decrypted MIME (receive_imf.rs:2375)
      const contact = this.contacts.find(c => c.email === msg.peerEmail);
      if (!contact?.pgpKey) {
        console.warn('Cannot send delete-for-all: no peer key');
        return;
      }

      // Need the rfc724mid (Message-ID) of the original message for Chat-Delete header
      const targetMid = msg.rfc724mid;
      if (!targetMid) {
        console.warn('Cannot send delete-for-all: no Message-ID for target message');
        return;
      }

      try {
        const msgId = `<${this.generateId()}@${this.credentials.email.split('@')[1]}>`;
        const now = new Date().toUTCString();

        // Chat-Delete goes INSIDE the encrypted body as a hidden header (mimefactory.rs:1057-1058 + mimeparser.rs:2228-2232)
        // The body text is 🚮 with hidden=true (core message.rs:1797-1805)
        const deleteBody = '🚮';
        const encryptedBody = await this.encryptMessage(deleteBody, contact.pgpKey, {
          from: this.credentials.email,
          to: msg.peerEmail,
          extraHeaders: `Chat-Delete: ${targetMid}`,
        });

        // Outer headers: Chat-Delete is NOT here (it's hidden inside PGP)
        const rawEmail = [
          this.buildFromHeader(),
          `To: <${msg.peerEmail}>`,
          `Date: ${now}`,
          `Message-ID: ${msgId}`,
          `Subject: [...]`,
          `Chat-Version: 1.0`,
        ];
        rawEmail.push(this.buildAutocryptHeader());
        rawEmail.push(
          `Content-Type: multipart/encrypted; protocol="application/pgp-encrypted"; boundary="pgp-del"`,
          `MIME-Version: 1.0`,
          '',
          '--pgp-del',
          'Content-Type: application/pgp-encrypted',
          '',
          'Version: 1',
          '',
          '--pgp-del',
          'Content-Type: application/octet-stream',
          '',
          encryptedBody,
          '',
          '--pgp-del--',
        );

        await this.api('POST', '/webimap/send', {
          from: this.credentials.email,
          to: [msg.peerEmail],
          body: rawEmail.join('\r\n'),
        });

        console.log('Delete-for-everyone request sent with Chat-Delete:', targetMid);
      } catch (e) {
        console.error('Delete-for-everyone failed:', e);
      }
    },

    // ---- Reactions (RFC 9078) ----

    getReactionCounts(msg) {
      if (!msg.reactions) return [];
      const counts = {};
      for (const emoji of Object.values(msg.reactions)) {
        if (emoji) counts[emoji] = (counts[emoji] || 0) + 1;
      }
      // Sort by frequency desc, then emoji
      return Object.entries(counts).sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
    },

    hasMyReaction(msg, emoji) {
      return msg.reactions && msg.reactions[this.credentials.email] === emoji;
    },

    async sendReaction(msg, emoji) {
      // If already reacted with same emoji, toggle it off (send empty reaction)
      const currentReaction = msg.reactions?.[this.credentials.email];
      const newReaction = (currentReaction === emoji) ? '' : emoji;

      const contact = this.contacts.find(c => c.email === msg.peerEmail);
      if (!contact?.pgpKey) {
        console.warn('Cannot send reaction: no peer key for', msg.peerEmail);
        this.toast('error', 'Cannot react: no encryption key for this contact');
        return;
      }

      const targetMid = msg.rfc724mid;
      if (!targetMid) {
        console.warn('Cannot send reaction: no Message-ID for target message, uid:', msg.uid);
        this.toast('error', 'Cannot react: message has no Message-ID');
        return;
      }

      console.log(`Sending reaction "${emoji}" for message ${targetMid} to ${msg.peerEmail}`);

      // Update local state immediately for responsive UI
      if (!msg.reactions) msg.reactions = {};
      if (newReaction) {
        msg.reactions[this.credentials.email] = newReaction;
      } else {
        delete msg.reactions[this.credentials.email];
      }
      await this.saveState();

      try {
        const msgId = `<${this.generateId()}@${this.credentials.email.split('@')[1]}>`;
        const now = new Date().toUTCString();

        // Reaction MIME: Content-Disposition: reaction (RFC 9078)
        // Both Content-Disposition and In-Reply-To go INSIDE the encrypted body
        // (core: Content-Disposition on the text part, In-Reply-To as protected header)
        const reactionText = newReaction || '';
        const encryptedBody = await this.encryptMessage(reactionText, contact.pgpKey, {
          from: this.credentials.email,
          to: msg.peerEmail,
          extraHeaders: [
            `Content-Disposition: reaction`,
            `In-Reply-To: ${targetMid}`,
          ].join('\r\n'),
        });

        // Outer headers: In-Reply-To is a "protected" header (both outer + inner, mimefactory.rs:1126)
        const rawEmail = [
          this.buildFromHeader(),
          `To: <${msg.peerEmail}>`,
          `Date: ${now}`,
          `Message-ID: ${msgId}`,
          `Subject: [...]`,
          `Chat-Version: 1.0`,
          `In-Reply-To: ${targetMid}`,
        ];
        rawEmail.push(this.buildAutocryptHeader());
        rawEmail.push(
          `Content-Type: multipart/encrypted; protocol="application/pgp-encrypted"; boundary="pgp-react"`,
          `MIME-Version: 1.0`,
          '',
          '--pgp-react',
          'Content-Type: application/pgp-encrypted',
          '',
          'Version: 1',
          '',
          '--pgp-react',
          'Content-Type: application/octet-stream',
          '',
          encryptedBody,
          '',
          '--pgp-react--',
        );

        await this.api('POST', '/webimap/send', {
          from: this.credentials.email,
          to: [msg.peerEmail],
          body: rawEmail.join('\r\n'),
        });

        console.log(`Reaction ${newReaction || '(removed)'} sent for message ${targetMid}`);
      } catch (e) {
        console.error('Reaction send failed:', e);
        this.toast('error', 'Failed to send reaction: ' + e.message);
        // Revert on failure
        if (currentReaction) {
          msg.reactions[this.credentials.email] = currentReaction;
        } else {
          delete msg.reactions[this.credentials.email];
        }
      }
    },

    openEmojiPicker(msg) {
      this.emojiPickerMsg = msg;
      this.emojiPickerOpen = true;
      this.$nextTick(() => {
        const container = this.$refs.emojiPickerContainer;
        if (!container) return;
        container.innerHTML = '';
        const picker = new EmojiMart.Picker({
          theme: 'dark',
          onEmojiSelect: (emoji) => {
            this.sendReaction(this.emojiPickerMsg, emoji.native);
            this.emojiPickerOpen = false;
          },
          set: 'native',
          perLine: 8,
          previewPosition: 'none',
          navPosition: 'bottom',
          skinTonePosition: 'none',
        });
        container.appendChild(picker);
      });
    },

    // ---- Messages ----
    get chatMessages() {
      if (!this.selectedContact) return [];
      const email = this.selectedContact.email;
      return this.allMessages.filter(m =>
        m.peerEmail === email
      ).sort((a, b) => new Date(a.date) - new Date(b.date));
    },

    // Handle image selection for compose
    handleComposeImage(event) {
      const file = event.target.files[0];
      if (!file) return;
      const reader = new FileReader();
      reader.onload = (e) => { this.composeImage = e.target.result; };
      reader.readAsDataURL(file);
    },

    async sendMessage() {
      if ((!this.composeText.trim() && !this.composeImage) || !this.selectedContact || this.sending) return;
      this.sending = true;
      const text = this.composeText.trim();
      const imageData = this.composeImage;
      this.composeText = '';
      this.composeImage = null;

      try {
        const to = this.selectedContact.email;

        // Prepare attachment if image selected
        let attachment = null;
        if (imageData) {
          const mimeMatch = imageData.match(/^data:([^;]+);base64,(.+)$/);
          if (mimeMatch) {
            const mimeType = mimeMatch[1];
            const ext = mimeType.split('/')[1] || 'jpg';
            attachment = {
              data: mimeMatch[2], // raw base64
              mimeType,
              filename: `image.${ext}`,
            };
          }
        }

        // Always encrypt — server enforces PGP-only policy
        let messageBody;
        let encrypted = false;

        const encOpts = { from: this.credentials.email, to, attachment };
        if (this.selectedContact.pgpKey) {
          messageBody = await this.encryptMessage(text, this.selectedContact.pgpKey, encOpts);
          encrypted = true;
        } else {
          messageBody = await this.encryptMessage(text, null, encOpts);
          encrypted = true;
        }

        const msgId = `<${this.generateId()}@${this.credentials.email.split('@')[1]}>`;
        const now = new Date().toUTCString();
        const rawEmail = [
          this.buildFromHeader(),
          `To: <${to}>`,
          `Date: ${now}`,
          `Message-ID: ${msgId}`,
          `Subject: [...]`,
          `Chat-Version: 1.0`,
        ];

        // Add Autocrypt header with proper folding
        rawEmail.push(this.buildAutocryptHeader());

        // Add Chat-User-Avatar header (Delta Chat convention: base64-encoded image data)
        if (this.profileImageB64) {
          // Extract raw base64 from data URL (remove 'data:image/jpeg;base64,' prefix)
          const avatarB64 = this.profileImageB64.replace(/^data:[^;]+;base64,/, '');
          rawEmail.push(`Chat-User-Avatar: base64:${avatarB64}`);
        }

        rawEmail.push(
          `Content-Type: multipart/encrypted; protocol="application/pgp-encrypted"; boundary="pgp-boundary"`,
          `MIME-Version: 1.0`,
          '',
          '--pgp-boundary',
          'Content-Type: application/pgp-encrypted',
          '',
          'Version: 1',
          '',
          '--pgp-boundary',
          'Content-Type: application/octet-stream',
          '',
          messageBody,
          '',
          '--pgp-boundary--',
        );

        await this.api('POST', '/webimap/send', {
          from: this.credentials.email,
          to: [to],
          body: rawEmail.join('\r\n'),
        });

        // Add to local view
        const sentUID = Date.now();
        this.allMessages.push({
          uid: sentUID,
          text,
          date: new Date().toISOString(),
          outgoing: true,
          encrypted,
          peerEmail: to,
          imageData: imageData || null,
          rfc724mid: msgId, // track Message-ID for delete-for-everyone
        });
        this.processedUIDs.add(sentUID);

        this.selectedContact.lastMsg = text.substring(0, 50);
        await this.saveState();
        this.$nextTick(() => this.scrollToBottom());
        this.toast('success', 'Message sent 🔒');

      } catch (e) {
        this.toast('error', 'Send failed: ' + e.message);
        this.composeText = text; // restore
      } finally {
        this.sending = false;
      }
    },

    // ---- WebSocket real-time messages ----
    connectWebSocket() {
      const wsProto = this.serverUrl.startsWith('https') ? 'wss' : 'ws';
      const host = this.serverUrl.replace(/^https?:\/\//, '');
      const wsUrl = `${wsProto}://${host}/webimap/ws?email=${encodeURIComponent(this.credentials.email)}&password=${encodeURIComponent(this.credentials.password)}&mailbox=INBOX&since_uid=${this.lastUID}`;

      this.ws = new WebSocket(wsUrl);
      this.polling = true;

      this.ws.onopen = () => {
        console.log('WebSocket connected');
      };

      this.ws.onmessage = async (event) => {
        try {
          const msg = JSON.parse(event.data);
          if (msg.uid > this.lastUID) this.lastUID = msg.uid;
          await this.processIncoming(msg);
        } catch (e) {
          console.error('Failed to process WS message:', e);
        }
      };

      this.ws.onclose = () => {
        console.log('WebSocket closed, reconnecting in 3s...');
        if (this.polling) {
          setTimeout(() => this.connectWebSocket(), 3000);
        }
      };

      this.ws.onerror = (e) => {
        console.error('WebSocket error:', e);
      };
    },

    disconnectWebSocket() {
      this.polling = false;
      if (this.ws) {
        this.ws.close();
        this.ws = null;
      }
    },

    // Parse raw MIME headers from message body
    parseRawHeaders(rawBody) {
      if (!rawBody) return {};
      // Split headers from body at the first blank line
      const sepIdx = rawBody.indexOf('\r\n\r\n');
      const sepIdx2 = rawBody.indexOf('\n\n');
      let headerSection;
      if (sepIdx >= 0 && (sepIdx2 < 0 || sepIdx < sepIdx2)) {
        headerSection = rawBody.substring(0, sepIdx);
      } else if (sepIdx2 >= 0) {
        headerSection = rawBody.substring(0, sepIdx2);
      } else {
        headerSection = rawBody;
      }

      // Unfold continuation lines (lines starting with space/tab)
      headerSection = headerSection.replace(/\r?\n[ \t]+/g, ' ');

      const headers = {};
      for (const line of headerSection.split(/\r?\n/)) {
        const colonIdx = line.indexOf(':');
        if (colonIdx > 0) {
          const key = line.substring(0, colonIdx).trim().toLowerCase();
          const value = line.substring(colonIdx + 1).trim();
          headers[key] = value;
        }
      }
      return headers;
    },

    async processIncoming(msg) {
      // Skip already processed by UID
      if (this.processedUIDs.has(msg.uid)) return;
      this.processedUIDs.add(msg.uid);

      // Also check by Message-ID to skip messages already in our local state
      // (e.g. our own sent messages echoed back from the server with a different UID)
      const msgHeaders = this.parseRawHeaders(msg.body || '');
      const incomingMid = msgHeaders['message-id'];
      if (incomingMid) {
        const midClean = incomingMid.replace(/^<|>$/g, '');
        const exists = this.allMessages.some(m => {
          const localMid = (m.rfc724mid || '').replace(/^<|>$/g, '');
          return localMid === midClean;
        });
        if (exists) {
          console.log(`Skipping duplicate message (Message-ID: ${incomingMid})`);
          return;
        }
      }

      try {
        // WebSocket sends full message detail (with body)
        const full = msg;

        // Parse raw MIME headers from the body
        const rawHeaders = this.parseRawHeaders(full.body);

        // Determine peer from envelope
        let peerEmail = '';
        if (full.envelope?.from?.length > 0) {
          const from = full.envelope.from[0];
          peerEmail = `${from.mailbox}@${from.host}`.toLowerCase();
        }

        // If it's from self, extract "to" as peer
        if (peerEmail === this.credentials.email.toLowerCase() && full.envelope?.to?.length > 0) {
          const to = full.envelope.to[0];
          peerEmail = `${to.mailbox}@${to.host}`.toLowerCase();
        }

        // Is this our own sent message?
        const isFromSelf = full.envelope?.from?.some(
          f => `${f.mailbox}@${f.host}`.toLowerCase() === this.credentials.email.toLowerCase()
        );

        // Try to decrypt the body
        let text = full.body || '';
        let encrypted = false;
        let innerHeaders = {}; // Headers from inside encrypted body

        // Check if body contains PGP encrypted data
        if (text.includes('-----BEGIN PGP MESSAGE-----')) {
          const pgpMatch = text.match(/-----BEGIN PGP MESSAGE-----[\s\S]*?-----END PGP MESSAGE-----/);
          if (pgpMatch) {
            const result = await this.decryptMessage(pgpMatch[0]);
            text = result.text;
            encrypted = result.encrypted;
            // Parse headers from the DECRYPTED content (Delta Chat puts Secure-Join etc inside PGP)
            innerHeaders = this.parseRawHeaders(text);
          }
        } else {
          // For non-encrypted messages, extract text from MIME body
          let bodyStart = full.body?.indexOf('\r\n\r\n');
          if (bodyStart < 0) bodyStart = full.body?.indexOf('\n\n');
          if (bodyStart >= 0) {
            text = full.body.substring(bodyStart + (full.body[bodyStart] === '\r' ? 4 : 2)).trim();
          }
        }

        // Merge outer + inner headers (inner takes priority for SecureJoin detection)
        const effectiveHeaders = { ...rawHeaders, ...innerHeaders };

        // Extract the actual text content and any image attachments
        let imageData = null;
        if (innerHeaders['content-type'] || innerHeaders['secure-join']) {
          const ct = (innerHeaders['content-type'] || '').toLowerCase();

          if (ct.includes('multipart/mixed')) {
            // Parse multipart/mixed to extract text + image parts
            const boundaryMatch = ct.match(/boundary="?([^";\s]+)"?/i);
            if (boundaryMatch) {
              const boundary = boundaryMatch[1];
              // Get body after inner headers
              let innerBodyStart = text.indexOf('\r\n\r\n');
              if (innerBodyStart < 0) innerBodyStart = text.indexOf('\n\n');
              const rawInner = innerBodyStart >= 0
                ? text.substring(innerBodyStart + (text[innerBodyStart] === '\r' ? 4 : 2))
                : text;

              // Split by boundary
              const parts = rawInner.split('--' + boundary);
              text = ''; // reset

              for (const part of parts) {
                const trimmed = part.trim();
                if (!trimmed || trimmed === '--') continue; // skip preamble/epilogue

                const partHeaders = this.parseRawHeaders(trimmed);
                const partCT = (partHeaders['content-type'] || '').toLowerCase();

                // Find body of this part (after headers)
                let partBodyStart = trimmed.indexOf('\r\n\r\n');
                if (partBodyStart < 0) partBodyStart = trimmed.indexOf('\n\n');
                const partBody = partBodyStart >= 0
                  ? trimmed.substring(partBodyStart + (trimmed[partBodyStart] === '\r' ? 4 : 2))
                  : '';

                if (partCT.includes('text/plain')) {
                  const partEncoding = (partHeaders['content-transfer-encoding'] || '').trim().toLowerCase();
                  if (partEncoding === 'base64') {
                    try { text = decodeURIComponent(escape(atob(partBody.replace(/\s/g, '')))); }
                    catch { text = atob(partBody.replace(/\s/g, '')); }
                  } else {
                    text = partBody.trim();
                  }
                } else if (partCT.includes('image/')) {
                  // Extract base64 image data
                  const encoding = (partHeaders['content-transfer-encoding'] || '').toLowerCase();
                  const b64Data = partBody.replace(/\s/g, '');
                  if (b64Data && encoding === 'base64') {
                    const mimeType = partCT.split(';')[0].trim();
                    imageData = `data:${mimeType};base64,${b64Data}`;
                  }
                }
              }
            }
          } else {
            // Simple text message with headers — extract body after inner headers
            let innerBodyStart = text.indexOf('\r\n\r\n');
            if (innerBodyStart < 0) innerBodyStart = text.indexOf('\n\n');
            if (innerBodyStart >= 0) {
              let bodyText = text.substring(innerBodyStart + (text[innerBodyStart] === '\r' ? 4 : 2)).trim();
              // Decode Content-Transfer-Encoding: base64 (used by Delta Chat core for reactions etc)
              const cte = (innerHeaders['content-transfer-encoding'] || '').trim().toLowerCase();
              if (cte === 'base64') {
                try { bodyText = decodeURIComponent(escape(atob(bodyText.replace(/\s/g, '')))); }
                catch { try { bodyText = atob(bodyText.replace(/\s/g, '')); } catch {} }
              }
              text = bodyText;
            }
          }
        }

        // Auto-create contact if new
        if (peerEmail && peerEmail !== this.credentials.email.toLowerCase()) {
          let contact = this.contacts.find(c => c.email === peerEmail);
          if (!contact) {
            contact = { email: peerEmail, pgpKey: null, fingerprint: null, verified: false, unread: 0, lastMsg: '' };
            this.contacts.push(contact);
          }

          // Extract and store Autocrypt key from BOTH outer and inner headers
          const autocryptHeader = effectiveHeaders['autocrypt'];
          if (autocryptHeader) {
            const keydataMatch = autocryptHeader.match(/keydata=([A-Za-z0-9+\/=\s]+)/i);
            if (keydataMatch) {
              try {
                const keydata = keydataMatch[1].replace(/\s/g, '');
                const keyArmored = `-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n${keydata}\n-----END PGP PUBLIC KEY BLOCK-----`;
                await openpgp.readKey({ armoredKey: keyArmored });
                contact.pgpKey = keyArmored;
                console.log(`Got valid Autocrypt key from ${peerEmail}`);
              } catch (e) {
                console.warn('Failed to parse Autocrypt key:', e);
              }
            }
          }

          // Also check Autocrypt-Gossip for our own key (used by Delta Chat in encrypted messages)
          const gossipHeader = effectiveHeaders['autocrypt-gossip'];
          if (gossipHeader && !autocryptHeader) {
            const keydataMatch = gossipHeader.match(/keydata=([A-Za-z0-9+\/=\s]+)/i);
            if (keydataMatch) {
              try {
                const keydata = keydataMatch[1].replace(/\s/g, '');
                const keyArmored = `-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n${keydata}\n-----END PGP PUBLIC KEY BLOCK-----`;
                await openpgp.readKey({ armoredKey: keyArmored });
                contact.pgpKey = keyArmored;
                console.log(`Got valid Autocrypt-Gossip key from ${peerEmail}`);
              } catch (e) {
                console.warn('Failed to parse Autocrypt-Gossip key:', e);
              }
            }
          }

          // Handle SecureJoin handshake messages (check both outer AND inner headers)
          const secureJoinStep = effectiveHeaders['secure-join'];
          if (secureJoinStep) {
            const step = secureJoinStep.trim().toLowerCase();
            console.log(`SecureJoin step received: ${step} from ${peerEmail}`);

            if (step === 'vc-request' || step === 'vg-request') {
              // We are Alice — someone scanned our QR
              contact.pendingVerification = true;
              this.toast('info', `🔐 ${peerEmail} wants to verify with you`);

            } else if (step === 'vc-auth-required' || step === 'vg-auth-required') {
              // We are Bob (Step 4) — Alice acknowledged our request, now send request-with-auth
              console.log(`vc-auth-required: contact.securejoinAuth=${contact.securejoinAuth}, contact.pgpKey=${!!contact.pgpKey}, contact.email=${contact.email}`);
              // Debug: check all contacts for this auth token
              const authContact = this.contacts.find(c => c.securejoinAuth);
              if (authContact && authContact.email !== contact.email) {
                console.log(`Auth token found on different contact: ${authContact.email} vs ${contact.email}`);
                // Transfer auth data to the correct contact (email format mismatch from QR)
                contact.securejoinAuth = authContact.securejoinAuth;
                contact.securejoinInvite = authContact.securejoinInvite;
                contact.fingerprint = authContact.fingerprint;
              }

              if (contact.securejoinAuth) {
                if (contact.pgpKey) {
                  this.toast('info', `🔐 Sending auth to ${peerEmail}...`);
                  try {
                    await this.sendSecureJoinAuth(peerEmail, contact.securejoinAuth, step.startsWith('vg'));
                    this.toast('success', `🔐 Auth sent to ${peerEmail}, waiting for confirmation...`);
                  } catch (e) {
                    console.error('sendSecureJoinAuth failed:', e);
                    this.toast('error', `SecureJoin auth failed: ${e.message}`);
                  }
                } else {
                  console.warn('Have auth token but no peer PGP key — cannot encrypt vc-request-with-auth');
                  this.toast('error', `SecureJoin: need encryption key for ${peerEmail}`);
                }
              } else {
                console.warn('Missing securejoin auth token — did you scan a QR code for this contact?');
                this.toast('error', `SecureJoin: no auth token for ${peerEmail}. Scan QR code first.`);
              }

            } else if (step === 'vc-contact-confirm' || step === 'vg-member-added') {
              // Alice confirmed — we are verified!
              contact.verified = true;
              this.toast('success', `✅ Verified contact: ${peerEmail}`);
            }
          }

          // Update contact UI state
          if (!secureJoinStep) {
            contact.lastMsg = imageData ? (text ? `📷 ${text.substring(0, 40)}` : '📷 Photo') : text.substring(0, 50);
            if (this.selectedContact?.email !== peerEmail) {
              contact.unread = (contact.unread || 0) + 1;
            }
          }
        }

        // Handle delete-request messages (Chat-Delete header — receive_imf.rs:2375)
        const chatDeleteHeader = effectiveHeaders['chat-delete'];
        const isDeleteRequest = !!chatDeleteHeader;
        if (isDeleteRequest) {
          console.log(`Chat-Delete received from ${peerEmail}: ${chatDeleteHeader}`);
          // Remove messages whose rfc724mid matches the Chat-Delete value
          const deleteTargets = chatDeleteHeader.trim().split(/\s+/);
          for (const target of deleteTargets) {
            const tid = target.replace(/^<|>$/g, ''); // strip angle brackets
            const idx = this.allMessages.findIndex(m => {
              const mid = (m.rfc724mid || '').replace(/^<|>$/g, '');
              return mid === tid;
            });
            if (idx >= 0) {
              console.log(`Deleting message with rfc724mid ${target} per Chat-Delete request`);
              this.allMessages.splice(idx, 1);
            }
          }
          await this.saveState();
        }

        // Handle incoming reactions (Content-Disposition: reaction — RFC 9078, mimeparser.rs:1341-1342)
        const contentDisp = (effectiveHeaders['content-disposition'] || '').trim().toLowerCase();
        const isReaction = contentDisp === 'reaction';
        if (isReaction) {
          const inReplyTo = (effectiveHeaders['in-reply-to'] || rawHeaders['in-reply-to'] || '').trim();
          const reactionEmoji = text.trim();
          console.log(`Reaction received from ${peerEmail}: "${reactionEmoji}" for ${inReplyTo}`);

          if (inReplyTo) {
            // Find the message being reacted to by rfc724mid
            const targetTid = inReplyTo.replace(/^<|>$/g, '');
            const targetMsg = this.allMessages.find(m => {
              const mid = (m.rfc724mid || '').replace(/^<|>$/g, '');
              return mid === targetTid;
            });
            if (targetMsg) {
              if (!targetMsg.reactions) targetMsg.reactions = {};
              if (reactionEmoji) {
                targetMsg.reactions[peerEmail] = reactionEmoji;
              } else {
                // Empty reaction = retract (core reaction.rs:14)
                delete targetMsg.reactions[peerEmail];
              }
              await this.saveState();
              console.log(`Applied reaction "${reactionEmoji}" from ${peerEmail} to message ${inReplyTo}`);
            }
          }
        }

        // Only add visible (non-handshake, non-system) messages to the chat
        const isHandshake = !!effectiveHeaders['secure-join'];
        const isHidden = isHandshake || isDeleteRequest || isReaction;
        if (!isHidden && !isFromSelf) {
          // Only add incoming messages — outgoing are already added at send time
          this.allMessages.push({
            uid: msg.uid,
            text,
            date: msg.date || full.date || new Date().toISOString(),
            outgoing: false,
            encrypted,
            peerEmail,
            imageData,
            rfc724mid: rawHeaders['message-id'] || null, // track for delete-for-everyone
          });

          if (this.selectedContact?.email === peerEmail) {
            this.$nextTick(() => this.scrollToBottom());
          }

          this.toast('info', `New message from ${peerEmail}`);
          await this.saveState();
        }

      } catch (e) {
        console.error('Failed to process message:', e);
      }
    },

    // Send vc-request-with-auth (Bob Step 4b)
    async sendSecureJoinAuth(toEmail, authToken, isGroup) {
      const step = isGroup ? 'vg-request-with-auth' : 'vc-request-with-auth';
      const msgId = `<${this.generateId()}@${this.credentials.email.split('@')[1]}>`;
      const now = new Date().toUTCString();

      // This message MUST be encrypted
      const bodyText = `Secure-Join: ${step}`;
      const contact = this.contacts.find(c => c.email === toEmail.toLowerCase());
      if (!contact?.pgpKey) {
        throw new Error('Cannot send auth: no recipient key');
      }

      const encryptedBody = await this.encryptMessage(bodyText, contact.pgpKey, {
        from: this.credentials.email,
        to: toEmail,
        extraHeaders: [
          `Secure-Join: ${step}`,
          `Secure-Join-Auth: ${authToken}`,
          `Secure-Join-Fingerprint: ${this.pgpFingerprint}`,
        ].join('\r\n'),
      });

      const rawEmail = [
        this.buildFromHeader(),
        `To: <${toEmail}>`,
        `Date: ${now}`,
        `Message-ID: ${msgId}`,
        `Subject: [...]`,
        `Chat-Version: 1.0`,
        `Secure-Join: ${step}`,
        `Secure-Join-Auth: ${authToken}`,
        `Secure-Join-Fingerprint: ${this.pgpFingerprint}`,
      ];

      rawEmail.push(this.buildAutocryptHeader());

      rawEmail.push(
        `Content-Type: multipart/encrypted; protocol="application/pgp-encrypted"; boundary="pgp-auth"`,
        `MIME-Version: 1.0`,
        '',
        '--pgp-auth',
        'Content-Type: application/pgp-encrypted',
        '',
        'Version: 1',
        '',
        '--pgp-auth',
        'Content-Type: application/octet-stream',
        '',
        encryptedBody,
        '',
        '--pgp-auth--',
      );

      await this.api('POST', '/webimap/send', {
        from: this.credentials.email,
        to: [toEmail],
        body: rawEmail.join('\r\n'),
      });
    },

    // ---- Utilities ----
    scrollToBottom() {
      const el = this.$refs.chatArea;
      if (el) el.scrollTop = el.scrollHeight;
    },

    formatTime(dateStr) {
      try {
        const d = new Date(dateStr);
        return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
      } catch { return ''; }
    },

    sleep(ms) {
      return new Promise(r => setTimeout(r, ms));
    },

    toast(type, message) {
      console.log(`[${type}] ${message}`);
    },
  };
}
