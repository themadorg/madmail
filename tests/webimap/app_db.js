/**
 * app_db.js — IndexedDB persistence layer for Delta Chat Web
 *
 * All database interactions go through this module.
 * The app component calls AppDB.open(), AppDB.save(data), AppDB.load(), AppDB.clear().
 */

const AppDB = (() => {
  const DB_NAME = 'madmail_chat';
  const DB_VERSION = 2;
  const STORE = 'state';
  const KEY = 'session';

  let _db = null;

  /**
   * Open (or create) the IndexedDB database.
   * @returns {Promise<void>}
   */
  async function open() {
    return new Promise((resolve) => {
      const req = indexedDB.open(DB_NAME, DB_VERSION);
      req.onupgradeneeded = (e) => {
        const db = e.target.result;
        if (!db.objectStoreNames.contains(STORE)) {
          db.createObjectStore(STORE);
        }
      };
      req.onsuccess = (e) => { _db = e.target.result; resolve(); };
      req.onerror = (e) => { console.warn('IndexedDB open failed:', e); resolve(); };
    });
  }

  /**
   * Serialize and save the full app state to IndexedDB.
   * @param {Object} state - Plain object with all fields to persist.
   * @returns {Promise<void>}
   */
  async function save(state) {
    if (!_db) return;
    try {
      const tx = _db.transaction(STORE, 'readwrite');
      const store = tx.objectStore(STORE);
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
          lastMsg: c.lastMsg || '',
          displayName: c.displayName || '',
          securejoinAuth: c.securejoinAuth || null,
          securejoinInvite: c.securejoinInvite || null,
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
        })),
        lastUID: state.lastUID || 0,
        securejoinUri: state.securejoinUri || '',
      }));
      store.put(data, KEY);
      return new Promise((resolve) => {
        tx.oncomplete = resolve;
        tx.onerror = () => resolve();
      });
    } catch (e) {
      console.warn('AppDB.save failed:', e);
    }
  }

  /**
   * Load persisted state from IndexedDB.
   * @returns {Promise<Object|null>} The saved state, or null if none found.
   */
  async function load() {
    if (!_db) return null;
    return new Promise((resolve) => {
      const tx = _db.transaction(STORE, 'readonly');
      const store = tx.objectStore(STORE);
      const req = store.get(KEY);
      req.onsuccess = () => {
        const data = req.result;
        if (!data?.credentials?.email || !data.privateKeyArmored) {
          resolve(null);
        } else {
          resolve(data);
        }
      };
      req.onerror = () => resolve(null);
    });
  }

  /**
   * Clear all persisted state.
   * @returns {Promise<void>}
   */
  async function clear() {
    if (!_db) return;
    const tx = _db.transaction(STORE, 'readwrite');
    tx.objectStore(STORE).delete(KEY);
    return new Promise((resolve) => {
      tx.oncomplete = resolve;
      tx.onerror = () => resolve();
    });
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
