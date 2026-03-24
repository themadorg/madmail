/**
 * app_db.js — Dexie.js persistence layer for Delta Chat Web
 *
 * All database interactions go through this module.
 * The app component calls AppDB.open(), AppDB.save(data), AppDB.load(), AppDB.clear().
 */

const AppDB = (() => {
  const DB_NAME = 'madmail_chat';
  const KEY = 'session';

  let _db = null;

  /**
   * Open (or create) the Dexie database.
   * @returns {Promise<void>}
   */
  async function open() {
    try {
      _db = new Dexie(DB_NAME);
      _db.version(3).stores({
        state: '' // outbound key (we use put with explicit key)
      });
      await _db.open();
    } catch (e) {
      if (typeof AppLog !== 'undefined') AppLog.warn('DB', 'Dexie open failed: ' + e.message);
      _db = null;
    }
  }

  /**
   * Serialize and save the full app state.
   * @param {Object} state - Plain object with all fields to persist.
   * @returns {Promise<void>}
   */
  async function save(state) {
    if (!_db) return;
    try {
      // JSON round-trip strips Alpine.js Proxy wrappers (not structured-cloneable)
      const data = JSON.parse(JSON.stringify({
        credentials: {
          email: state.credentials?.email || '',
          password: state.credentials?.password || '',
        },
        displayName: state.displayName || '',
        profileImageB64: state.profileImageB64 || '',
        publicKeyArmored: state.publicKeyArmored || '',
        privateKeyArmored: state.privateKeyArmored || '',
        pgpFingerprint: state.pgpFingerprint || '',
        autocryptKeydata: state.autocryptKeydata || '',
        contacts: (state.contacts || []).map(c => ({
          email: c.email,
          pgpKey: c.pgpKey || null,
          fingerprint: c.fingerprint || null,
          verified: !!c.verified,
          pendingVerification: !!c.pendingVerification,
          unread: c.unread || 0,
          lastMsg: c.lastMsg || '',
          displayName: c.displayName || '',
          securejoinAuth: c.securejoinAuth || null,
          securejoinInvite: c.securejoinInvite || null,
        })),
        channels: (state.channels || []).map(ch => ({
          id: ch.id,
          name: ch.name || '',
          inviteUri: ch.inviteUri || '',
          qrDataUrl: ch.qrDataUrl || '',
          inviteNumber: ch.inviteNumber || '',
          auth: ch.auth || '',
          createdBy: ch.createdBy || '',
          isBroadcast: !!ch.isBroadcast,
          role: ch.role || 'subscriber',
          status: ch.status || 'joined',
          members: ch.members || [],
        })),
        allMessages: (state.allMessages || []).slice(-500).map(m => ({
          uid: m.uid,
          text: m.text || '',
          date: m.date,
          outgoing: !!m.outgoing,
          encrypted: !!m.encrypted,
          peerEmail: m.peerEmail,
          imageData: m.imageData || null,
          rfc724mid: m.rfc724mid || null,
          reactions: m.reactions || null,
          channelId: m.channelId || null,
        })),
        lastUID: state.lastUID || 0,
        securejoinUri: state.securejoinUri || '',
      }));
      await _db.state.put(data, KEY);
    } catch (e) {
      if (typeof AppLog !== 'undefined') AppLog.warn('DB', 'AppDB.save failed: ' + e.message);
    }
  }

  /**
   * Load persisted state.
   * @returns {Promise<Object|null>} The saved state, or null if none found.
   */
  async function load() {
    if (!_db) return null;
    try {
      const data = await _db.state.get(KEY);
      if (!data?.credentials?.email || !data.privateKeyArmored) {
        return null;
      }
      return data;
    } catch (e) {
      if (typeof AppLog !== 'undefined') AppLog.warn('DB', 'AppDB.load failed: ' + e.message);
      return null;
    }
  }

  /**
   * Clear all persisted state.
   * @returns {Promise<void>}
   */
  async function clear() {
    if (!_db) return;
    try {
      await _db.state.delete(KEY);
    } catch (e) {
      if (typeof AppLog !== 'undefined') AppLog.warn('DB', 'AppDB.clear failed: ' + e.message);
    }
  }

  /**
   * Check if DB is connected.
   * @returns {boolean}
   */
  function isConnected() {
    return !!_db;
  }

  return { open, save, load, clear, isConnected };
})();
