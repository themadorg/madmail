package module

import "sync/atomic"

// sentMessages is the global counter for successfully sent messages.
// It is incremented atomically from SMTP endpoints after each successful
// delivery commit, and periodically flushed to the database by the
// storage module.
var sentMessages atomic.Int64

// outboundMessages counts messages successfully delivered to external servers.
var outboundMessages atomic.Int64

// IncrementSentMessages atomically adds 1 to the global sent message counter.
func IncrementSentMessages() {
	sentMessages.Add(1)
}

// GetSentMessages returns the current value of the global counter.
func GetSentMessages() int64 {
	return sentMessages.Load()
}

// SetSentMessages sets the counter to a specific value.
// Used by the storage module to restore the persisted count on startup.
func SetSentMessages(n int64) {
	sentMessages.Store(n)
}

// IncrementOutboundMessages atomically adds 1 to the outbound message counter.
func IncrementOutboundMessages() {
	outboundMessages.Add(1)
}

// GetOutboundMessages returns the current outbound count.
func GetOutboundMessages() int64 {
	return outboundMessages.Load()
}

// SetOutboundMessages sets the outbound counter to a specific value.
func SetOutboundMessages(n int64) {
	outboundMessages.Store(n)
}

// receivedMessages counts messages received from external servers.
var receivedMessages atomic.Int64

// IncrementReceivedMessages atomically adds 1 to the received counter.
func IncrementReceivedMessages() {
	receivedMessages.Add(1)
}

// GetReceivedMessages returns the current received count.
func GetReceivedMessages() int64 {
	return receivedMessages.Load()
}

// SetReceivedMessages sets the received counter to a specific value.
func SetReceivedMessages(n int64) {
	receivedMessages.Store(n)
}
