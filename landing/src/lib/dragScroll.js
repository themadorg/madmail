/** Mouse dragging supplements native touch, trackpad and keyboard scrolling. */
export function dragScroll(node) {
	let pointer = null;
	let startX = 0;
	let startScroll = 0;
	let dragged = false;
	let suppressClick = false;
	function down(event) {
		if (event.pointerType !== 'mouse') { suppressClick = false; return; }
		if (event.button !== 0) return;
		pointer = event.pointerId;
		startX = event.clientX;
		startScroll = node.scrollLeft;
		dragged = false;
		suppressClick = false;
	}
	function move(event) {
		if (event.pointerId !== pointer) return;
		if (!dragged && Math.abs(event.clientX - startX) > 5) {
			dragged = true;
			node.focus({ preventScroll: true });
			node.classList.add('is-dragging');
			node.setPointerCapture(pointer);
		}
		if (dragged) node.scrollLeft = startScroll - (event.clientX - startX);
	}
	function end(event) {
		if (event.pointerId !== pointer) return;
		const captured = pointer;
		pointer = null;
		suppressClick = dragged;
		node.classList.remove('is-dragging');
		if (node.hasPointerCapture(captured)) node.releasePointerCapture(captured);
	}
	function click(event) {
		if (!suppressClick || event.detail === 0) return;
		event.preventDefault();
		event.stopImmediatePropagation();
		suppressClick = false;
	}
	function preventDrag(event) { event.preventDefault(); }
	const handlers = { pointerdown: down, pointermove: move, pointerup: end,
		pointercancel: end, lostpointercapture: end, dragstart: preventDrag };
	for (const [name, handler] of Object.entries(handlers)) node.addEventListener(name, handler);
	node.addEventListener('click', click, true);
	window.addEventListener('pointerup', end);
	return { destroy() {
		for (const [name, handler] of Object.entries(handlers)) node.removeEventListener(name, handler);
		node.removeEventListener('click', click, true);
		window.removeEventListener('pointerup', end);
		node.classList.remove('is-dragging');
	} };
}
