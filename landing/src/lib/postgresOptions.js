/** Enhance only guides that explicitly provide PostgreSQL setup choices. */
export function initPostgresOptions(root) {
	const controls = root.querySelector('[data-postgres-options]');
	if (!controls || controls.dataset.initialized) return;
	controls.dataset.initialized = 'true';
	const selection = { os: 'debian', method: 'docker' };
	const buttons = controls.querySelectorAll('[data-pg-choice]');
	function update() {
		for (const button of buttons) {
			button.setAttribute('aria-pressed', String(selection[button.dataset.pgChoice] === button.dataset.pgValue));
		}
		for (const panel of root.querySelectorAll('[data-pg-method], [data-pg-os]')) {
			panel.hidden = Boolean(panel.dataset.pgMethod && panel.dataset.pgMethod !== selection.method
				|| panel.dataset.pgOs && panel.dataset.pgOs !== selection.os);
		}
	}
	for (const button of buttons) {
		button.addEventListener('click', () => {
			selection[button.dataset.pgChoice] = button.dataset.pgValue;
			update();
		});
	}
	update();
}
