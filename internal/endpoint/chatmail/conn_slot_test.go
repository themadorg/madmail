package chatmail

import "testing"

func TestConnSlotHelpers(t *testing.T) {
	t.Parallel()

	if !acquireConnSlot(nil) {
		t.Fatal("expected nil semaphore acquire to succeed")
	}
	releaseConnSlot(nil)

	sem := make(chan struct{}, 1)
	if !acquireConnSlot(sem) {
		t.Fatal("expected first acquire to succeed")
	}
	if acquireConnSlot(sem) {
		t.Fatal("expected second acquire to fail when capacity is full")
	}

	releaseConnSlot(sem)
	if !acquireConnSlot(sem) {
		t.Fatal("expected acquire to succeed after release")
	}
}
