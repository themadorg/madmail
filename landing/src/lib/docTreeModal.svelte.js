export const docTreeModal = $state({ open: false, docked: true, width: 272, focusDirId: null });

/** @param {string | null} dirId */
export function openDocTreeAtDir(dirId) {
	docTreeModal.focusDirId = dirId;
	docTreeModal.open = true;
}

export function openDocTree() {
	openDocTreeAtDir(null);
}

export function closeDocTree() {
	docTreeModal.open = false;
	docTreeModal.focusDirId = null;
}

export function toggleDocTreeDock() {
	docTreeModal.docked = !docTreeModal.docked;
}
