package db

import "testing"

func TestAugmentModerncSqliteDSN(t *testing.T) {
	t.Parallel()
	cases := []struct {
		in, want string
	}{
		{"", ""},
		{
			"credentials.db",
			"credentials.db?_pragma=busy_timeout(30000)&_pragma=journal_mode(WAL)",
		},
		{
			"file:foo.db?_pragma=busy_timeout(1000)",
			"file:foo.db?_pragma=busy_timeout(1000)",
		},
		{
			"file:foo.db?cache=shared",
			"file:foo.db?cache=shared&_pragma=busy_timeout(30000)&_pragma=journal_mode(WAL)",
		},
	}
	for _, tc := range cases {
		got := augmentModerncSqliteDSN(tc.in)
		if got != tc.want {
			t.Errorf("augmentModerncSqliteDSN(%q) = %q, want %q", tc.in, got, tc.want)
		}
	}
}
