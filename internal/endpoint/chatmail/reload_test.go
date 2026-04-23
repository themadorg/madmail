package chatmail

import "testing"

func TestReplaceAddrPort(t *testing.T) {
	tests := []struct {
		addr, port, want string
		ok                bool
	}{
		{"tls://0.0.0.0:443", "8080", "tls://0.0.0.0:8080", true},
		{"tcp://0.0.0.0:80", "8081", "tcp://0.0.0.0:8081", true},
		{"tls://[::]:443", "9443", "tls://[::]:9443", true},
		{"nope", "80", "", false},
	}
	for _, tc := range tests {
		got, ok := replaceAddrPort(tc.addr, tc.port)
		if ok != tc.ok {
			t.Errorf("replaceAddrPort(%q, %q) ok = %v, want %v", tc.addr, tc.port, ok, tc.ok)
			continue
		}
		if ok && got != tc.want {
			t.Errorf("replaceAddrPort(%q, %q) = %q, want %q", tc.addr, tc.port, got, tc.want)
		}
	}
}
