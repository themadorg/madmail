// IndexedDB-backed store for saved server credentials.
// Allows admins to save and switch between multiple Madmail servers.

export interface SavedServer {
    id: string;        // auto-generated unique ID
    label: string;     // user-defined label (e.g. "Production", "Staging")
    url: string;       // base URL
    token: string;     // admin token
    addedAt: number;   // timestamp
}

const DB_NAME = 'madmail_admin';
const DB_VERSION = 1;
const STORE_NAME = 'servers';

function openDB(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
        const req = indexedDB.open(DB_NAME, DB_VERSION);
        req.onupgradeneeded = () => {
            const db = req.result;
            if (!db.objectStoreNames.contains(STORE_NAME)) {
                db.createObjectStore(STORE_NAME, { keyPath: 'id' });
            }
        };
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => reject(req.error);
    });
}

export async function getSavedServers(): Promise<SavedServer[]> {
    try {
        const db = await openDB();
        return new Promise((resolve, reject) => {
            const tx = db.transaction(STORE_NAME, 'readonly');
            const store = tx.objectStore(STORE_NAME);
            const req = store.getAll();
            req.onsuccess = () => {
                const servers = (req.result as SavedServer[])
                    .sort((a, b) => b.addedAt - a.addedAt);
                resolve(servers);
            };
            req.onerror = () => reject(req.error);
        });
    } catch {
        return [];
    }
}

export async function saveServer(url: string, token: string, label?: string): Promise<SavedServer> {
    const db = await openDB();
    // Check if a server with the same URL already exists
    const existing = await getSavedServers();
    const match = existing.find(s => s.url === url);

    const server: SavedServer = {
        id: match?.id ?? crypto.randomUUID(),
        label: label ?? match?.label ?? extractLabel(url),
        url,
        token,
        addedAt: match?.addedAt ?? Date.now(),
    };

    return new Promise((resolve, reject) => {
        const tx = db.transaction(STORE_NAME, 'readwrite');
        const store = tx.objectStore(STORE_NAME);
        store.put(server);
        tx.oncomplete = () => resolve(server);
        tx.onerror = () => reject(tx.error);
    });
}

export async function removeServer(id: string): Promise<void> {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const tx = db.transaction(STORE_NAME, 'readwrite');
        const store = tx.objectStore(STORE_NAME);
        store.delete(id);
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
    });
}

export async function updateServerLabel(id: string, label: string): Promise<void> {
    const db = await openDB();
    const servers = await getSavedServers();
    const server = servers.find(s => s.id === id);
    if (!server) return;
    server.label = label;
    return new Promise((resolve, reject) => {
        const tx = db.transaction(STORE_NAME, 'readwrite');
        const store = tx.objectStore(STORE_NAME);
        store.put(server);
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
    });
}

function extractLabel(url: string): string {
    try {
        const u = new URL(url);
        return u.hostname;
    } catch {
        return url;
    }
}
