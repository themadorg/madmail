package iroh

import _ "embed"

//go:embed assets/iroh-relay
var IrohRelayBinary []byte

//go:embed assets/VERSION
var IrohRelayVersion string
