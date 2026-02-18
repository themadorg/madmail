package chatmail

import (
	"strings"
	"testing"

	"github.com/themadorg/madmail/internal/api/admin/resources"
)

// sampleConfig is a simplified maddy.conf for testing regex replacements.
const sampleConfig = `
smtp tcp://0.0.0.0:25 {
    limits {
        all rate 20 1s
    }
}

submission tls://0.0.0.0:465 tcp://0.0.0.0:587 {
    auth &local_authdb
}

imap tls://0.0.0.0:993 tcp://0.0.0.0:143 {
    turn_port 3478
    turn_secret mysecret123
    turn_ttl 86400
    iroh_relay_url http://1.2.3.4:3340
}

turn udp://0.0.0.0:3478 tcp://0.0.0.0:3478 {
    realm 1.2.3.4
    secret mysecret123
    relay_ip 1.2.3.4
}

chatmail tls://0.0.0.0:443 {
    ss_addr "0.0.0.0:8388"
    ss_password "defaultpass"
    ss_cipher "aes-128-gcm"
}
`

func TestApplyPortOverride_SMTPPort(t *testing.T) {
	for _, m := range configOverrides {
		if m.dbKey == resources.KeySMTPPort {
			result := applyPortOverride(sampleConfig, m, "2525")
			if !strings.Contains(result, "smtp tcp://0.0.0.0:2525") {
				t.Errorf("SMTP port not updated. Got:\n%s", result)
			}
			if strings.Contains(result, "smtp tcp://0.0.0.0:25 ") {
				t.Errorf("Old SMTP port still present")
			}
			return
		}
	}
	t.Fatal("KeySMTPPort mapping not found")
}

func TestApplyPortOverride_SubmissionPort(t *testing.T) {
	for _, m := range configOverrides {
		if m.dbKey == resources.KeySubmissionPort {
			result := applyPortOverride(sampleConfig, m, "1587")
			if !strings.Contains(result, "tcp://0.0.0.0:1587") {
				t.Errorf("Submission port not updated. Got:\n%s", result)
			}
			// The TLS port should remain unchanged
			if !strings.Contains(result, "tls://0.0.0.0:465") {
				t.Errorf("Submission TLS port was incorrectly modified")
			}
			return
		}
	}
	t.Fatal("KeySubmissionPort mapping not found")
}

func TestApplyPortOverride_IMAPPort(t *testing.T) {
	for _, m := range configOverrides {
		if m.dbKey == resources.KeyIMAPPort {
			result := applyPortOverride(sampleConfig, m, "1993")
			if !strings.Contains(result, "tcp://0.0.0.0:1993") {
				t.Errorf("IMAP port not updated. Got:\n%s", result)
			}
			// TLS port should remain
			if !strings.Contains(result, "tls://0.0.0.0:993") {
				t.Errorf("IMAP TLS port was incorrectly modified")
			}
			return
		}
	}
	t.Fatal("KeyIMAPPort mapping not found")
}

func TestApplyPortOverride_TURNPort(t *testing.T) {
	for _, m := range configOverrides {
		if m.dbKey == resources.KeyTurnPort {
			result := applyPortOverride(sampleConfig, m, "5000")
			if !strings.Contains(result, "udp://0.0.0.0:5000") {
				t.Errorf("TURN UDP port not updated. Got:\n%s", result)
			}
			if !strings.Contains(result, "tcp://0.0.0.0:5000") {
				t.Errorf("TURN TCP port not updated. Got:\n%s", result)
			}
			return
		}
	}
	t.Fatal("KeyTurnPort mapping not found")
}

func TestApplyPortOverride_SsPort(t *testing.T) {
	for _, m := range configOverrides {
		if m.dbKey == resources.KeySsPort {
			result := applyPortOverride(sampleConfig, m, "9999")
			if !strings.Contains(result, `ss_addr "0.0.0.0:9999"`) {
				t.Errorf("SS port not updated. Got:\n%s", result)
			}
			return
		}
	}
	t.Fatal("KeySsPort mapping not found")
}

func TestApplyPortOverride_SsPassword(t *testing.T) {
	for _, m := range configOverrides {
		if m.dbKey == resources.KeySsPassword {
			result := applyPortOverride(sampleConfig, m, "newpass456")
			if !strings.Contains(result, `ss_password "newpass456"`) {
				t.Errorf("SS password not updated. Got:\n%s", result)
			}
			return
		}
	}
	t.Fatal("KeySsPassword mapping not found")
}

func TestApplyPortOverride_SsCipher(t *testing.T) {
	for _, m := range configOverrides {
		if m.dbKey == resources.KeySsCipher {
			result := applyPortOverride(sampleConfig, m, "aes-256-gcm")
			if !strings.Contains(result, `ss_cipher "aes-256-gcm"`) {
				t.Errorf("SS cipher not updated. Got:\n%s", result)
			}
			return
		}
	}
	t.Fatal("KeySsCipher mapping not found")
}

func TestApplyPortOverride_TurnSecret(t *testing.T) {
	for _, m := range configOverrides {
		if m.dbKey == resources.KeyTurnSecret {
			result := applyPortOverride(sampleConfig, m, "newsecret789")
			if !strings.Contains(result, "turn_secret newsecret789") {
				t.Errorf("TURN secret not updated in IMAP block. Got:\n%s", result)
			}
			return
		}
	}
	t.Fatal("KeyTurnSecret mapping not found")
}

func TestApplyPortOverride_IrohPort(t *testing.T) {
	for _, m := range configOverrides {
		if m.dbKey == resources.KeyIrohPort {
			result := applyPortOverride(sampleConfig, m, "4444")
			if !strings.Contains(result, "http://1.2.3.4:4444") {
				t.Errorf("Iroh port not updated. Got:\n%s", result)
			}
			return
		}
	}
	t.Fatal("KeyIrohPort mapping not found")
}

func TestApplyPortOverride_TurnTTL(t *testing.T) {
	for _, m := range configOverrides {
		if m.dbKey == resources.KeyTurnTTL {
			result := applyPortOverride(sampleConfig, m, "7200")
			if !strings.Contains(result, "turn_ttl 7200") {
				t.Errorf("TURN TTL not updated. Got:\n%s", result)
			}
			return
		}
	}
	t.Fatal("KeyTurnTTL mapping not found")
}

func TestConfigOverrideKeysComplete(t *testing.T) {
	keys := configOverrideKeys()
	expected := []string{
		resources.KeySMTPPort,
		resources.KeySubmissionPort,
		resources.KeyIMAPPort,
		resources.KeyTurnPort,
		resources.KeyIrohPort,
		resources.KeySsPort,
		resources.KeySsPassword,
		resources.KeySsCipher,
		resources.KeyTurnSecret,
		resources.KeyTurnRealm,
		resources.KeyTurnRelayIP,
		resources.KeyTurnTTL,
	}
	for _, k := range expected {
		if !keys[k] {
			t.Errorf("Missing config override key: %s", k)
		}
	}
}

func TestNoOverrideDoesNotModify(t *testing.T) {
	// Applying a port override where the pattern doesn't match should not modify content
	result := applyPortOverride("some random text without any config", configOverrides[0], "9999")
	if result != "some random text without any config" {
		t.Errorf("Content was modified when pattern didn't match")
	}
}
