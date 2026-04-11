package chatmail

import (
	"testing"
)

func TestValidateMxDelivTLS(t *testing.T) {
	// Bug #1: unencrypted HTTP delivery should be rejected
	tests := []struct {
		name    string
		isTLS   bool
		wantErr bool
	}{
		{"HTTPS connection accepted", true, false},
		{"HTTP connection rejected", false, true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateMxDelivTLS(tt.isTLS)
			if (err != nil) != tt.wantErr {
				t.Errorf("ValidateMxDelivTLS(%v) error = %v, wantErr %v", tt.isTLS, err, tt.wantErr)
			}
			if err != nil {
				secErr, ok := err.(*MxDelivSecurityError)
				if !ok {
					t.Fatalf("expected MxDelivSecurityError, got %T", err)
				}
				if secErr.Code != 403 {
					t.Errorf("expected code 403, got %d", secErr.Code)
				}
			}
		})
	}
}

func TestNormalizeMailDomain(t *testing.T) {
	tests := []struct {
		name       string
		mailDomain string
		wantLen    int
		wantFirst  string
	}{
		{"bracketed IP", "[1.1.1.1]", 2, "[1.1.1.1]"},
		{"bare IP", "1.1.1.1", 2, "1.1.1.1"},
		{"hostname", "example.com", 1, "example.com"},
		{"bracketed IPv6", "[::1]", 2, "[::1]"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			domains := NormalizeMailDomain(tt.mailDomain)
			if len(domains) != tt.wantLen {
				t.Errorf("NormalizeMailDomain(%q) got %d domains, want %d: %v", tt.mailDomain, len(domains), tt.wantLen, domains)
			}
			if domains[0] != tt.wantFirst {
				t.Errorf("NormalizeMailDomain(%q) first = %q, want %q", tt.mailDomain, domains[0], tt.wantFirst)
			}
		})
	}
}

func TestValidateRecipientDomain(t *testing.T) {
	// Bug #3: server should not accept emails for other domains
	validDomains := NormalizeMailDomain("[1.1.1.1]")

	tests := []struct {
		name    string
		rcptTo  string
		wantErr bool
	}{
		// Valid: recipient domain matches server
		{"bracketed IP matches", "user@[1.1.1.1]", false},
		{"bare IP matches", "user@1.1.1.1", false},

		// Invalid: recipient domain differs from server
		{"different IP rejected", "user@2.2.2.2", true},
		{"different bracketed IP rejected", "user@[2.2.2.2]", true},
		{"hostname rejected on IP server", "user@example.com", true},

		// Invalid: malformed addresses
		{"no at sign", "user", true},
		{"empty local part", "@1.1.1.1", false}, // has correct domain
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateRecipientDomain(tt.rcptTo, validDomains)
			if (err != nil) != tt.wantErr {
				t.Errorf("ValidateRecipientDomain(%q) error = %v, wantErr %v", tt.rcptTo, err, tt.wantErr)
			}
		})
	}
}

func TestValidateRecipientDomain_Hostname(t *testing.T) {
	// Test with hostname-based domain
	validDomains := NormalizeMailDomain("example.com")

	tests := []struct {
		name    string
		rcptTo  string
		wantErr bool
	}{
		{"matching domain", "user@example.com", false},
		{"different domain", "user@other.com", true},
		{"IP on hostname server", "user@1.1.1.1", true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateRecipientDomain(tt.rcptTo, validDomains)
			if (err != nil) != tt.wantErr {
				t.Errorf("ValidateRecipientDomain(%q) error = %v, wantErr %v", tt.rcptTo, err, tt.wantErr)
			}
		})
	}
}

func TestValidateRecipientNotAdmin(t *testing.T) {
	// Bug #2: admin addresses should be blocked from federation
	tests := []struct {
		name    string
		rcptTo  string
		wantErr bool
	}{
		{"admin blocked", "admin@[1.1.1.1]", true},
		{"Admin blocked (case)", "Admin@[1.1.1.1]", true},
		{"ADMIN blocked (upper)", "ADMIN@1.1.1.1", true},
		{"root blocked", "root@[1.1.1.1]", true},
		{"postmaster blocked", "postmaster@[1.1.1.1]", true},
		{"mailer-daemon blocked", "mailer-daemon@[1.1.1.1]", true},
		{"abuse blocked", "abuse@[1.1.1.1]", true},
		{"hostmaster blocked", "hostmaster@[1.1.1.1]", true},
		{"webmaster blocked", "webmaster@[1.1.1.1]", true},

		// Normal users should pass
		{"normal user passes", "alice@[1.1.1.1]", false},
		{"user123 passes", "user123@[1.1.1.1]", false},
		{"admin-like passes", "admin2@[1.1.1.1]", false},
		{"admin prefix passes", "administrator@[1.1.1.1]", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateRecipientNotAdmin(tt.rcptTo)
			if (err != nil) != tt.wantErr {
				t.Errorf("ValidateRecipientNotAdmin(%q) error = %v, wantErr %v", tt.rcptTo, err, tt.wantErr)
			}
		})
	}
}

func TestValidateAllRecipients(t *testing.T) {
	validDomains := NormalizeMailDomain("[1.1.1.1]")

	mailTo := []string{
		"alice@[1.1.1.1]", // valid
		"bob@1.1.1.1",     // valid (bare IP)
		"admin@[1.1.1.1]", // blocked admin
		"user@2.2.2.2",    // wrong domain
		"root@1.1.1.1",    // blocked admin
		"carol@[1.1.1.1]", // valid
	}

	accepted, rejected := ValidateAllRecipients(mailTo, validDomains)

	if len(accepted) != 3 {
		t.Errorf("expected 3 accepted, got %d: %v", len(accepted), accepted)
	}
	if len(rejected) != 3 {
		t.Errorf("expected 3 rejected, got %d: %v", len(rejected), rejected)
	}

	// Check specific accepted recipients
	expectedAccepted := map[string]bool{
		"alice@[1.1.1.1]": true,
		"bob@1.1.1.1":     true,
		"carol@[1.1.1.1]": true,
	}
	for _, a := range accepted {
		if !expectedAccepted[a] {
			t.Errorf("unexpected accepted recipient: %s", a)
		}
	}

	// Check specific rejected recipients
	expectedRejected := map[string]bool{
		"admin@[1.1.1.1]": true,
		"user@2.2.2.2":    true,
		"root@1.1.1.1":    true,
	}
	for r := range rejected {
		if !expectedRejected[r] {
			t.Errorf("unexpected rejected recipient: %s", r)
		}
	}
}

func TestValidateAllRecipients_AllRejected(t *testing.T) {
	validDomains := NormalizeMailDomain("[1.1.1.1]")

	mailTo := []string{
		"admin@[1.1.1.1]",
		"user@2.2.2.2",
	}

	accepted, rejected := ValidateAllRecipients(mailTo, validDomains)

	if len(accepted) != 0 {
		t.Errorf("expected 0 accepted, got %d: %v", len(accepted), accepted)
	}
	if len(rejected) != 2 {
		t.Errorf("expected 2 rejected, got %d", len(rejected))
	}
}

func TestValidateAllRecipients_AllAccepted(t *testing.T) {
	validDomains := NormalizeMailDomain("[1.1.1.1]")

	mailTo := []string{
		"alice@[1.1.1.1]",
		"bob@1.1.1.1",
	}

	accepted, rejected := ValidateAllRecipients(mailTo, validDomains)

	if len(accepted) != 2 {
		t.Errorf("expected 2 accepted, got %d: %v", len(accepted), accepted)
	}
	if len(rejected) != 0 {
		t.Errorf("expected 0 rejected, got %d", len(rejected))
	}
}

func TestMxDelivSecurityError(t *testing.T) {
	err := &MxDelivSecurityError{
		Code:    403,
		Message: "test error message",
	}

	expected := "mxdeliv security: test error message (code 403)"
	if err.Error() != expected {
		t.Errorf("Error() = %q, want %q", err.Error(), expected)
	}
}
