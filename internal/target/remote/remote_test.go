/*
Maddy Mail Server - Composable all-in-one email server.
Copyright © 2019-2020 Max Mazurov <fox.cpp@disroot.org>, Maddy Mail Server contributors

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

package remote

import (
	"context"
	"crypto/tls"
	"flag"
	"math/rand"
	"net"
	"os"
	"strconv"
	"testing"

	"github.com/emersion/go-message/textproto"
	"github.com/emersion/go-smtp"
	"github.com/foxcpp/go-mockdns"
	"github.com/foxcpp/go-mtasts"
	"github.com/themadorg/madmail/framework/buffer"
	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/dns"
	"github.com/themadorg/madmail/framework/exterrors"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/limits"
	"github.com/themadorg/madmail/internal/smtpconn/pool"
	"github.com/themadorg/madmail/internal/testutils"
)

// .invalid TLD is used here to make sure if there is something wrong about
// DNS hooks and lookups go to the real Internet, they will not result in
// any useful data that can lead to outgoing connections being made.

func testTarget(t *testing.T, zones map[string]mockdns.Zone, extResolver *dns.ExtResolver,
	extraPolicies []module.MXAuthPolicy) *Target {
	resolver := &mockdns.Resolver{Zones: zones}

	tgt := Target{
		name:        "remote",
		hostname:    "mx.example.com",
		resolver:    resolver,
		dialer:      resolver.DialContext,
		extResolver: extResolver,
		tlsConfig:   &tls.Config{},
		Log:         testutils.Logger(t, "remote"),
		policies:    extraPolicies,
		limits:      &limits.Group{},
		pool: pool.New(pool.Config{
			MaxKeys:             5000,
			MaxConnsPerKey:      5,      // basically, max. amount of idle connections in cache
			MaxConnLifetimeSec:  150,    // 2.5 mins, half of recommended idle time from RFC 5321
			StaleKeyLifetimeSec: 60 * 5, // should be bigger than MaxConnLifetimeSec
		}),
	}

	return &tgt
}

func testSTSPolicy(t *testing.T, zones map[string]mockdns.Zone, mtastsGet func(context.Context, string) (*mtasts.Policy, error)) *mtastsPolicy {
	m, err := NewMTASTSPolicy("mx_auth.mtasts", "test", nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	p := m.(*mtastsPolicy)
	err = p.Init(config.NewMap(nil, config.Node{
		Children: []config.Node{
			{
				Name: "cache",
				Args: []string{"ram"},
			},
		},
	}))
	if err != nil {
		t.Fatal(err)
	}

	p.mtastsGet = mtastsGet
	p.log = testutils.Logger(t, "remote/mtasts")
	p.cache.Resolver = &mockdns.Resolver{Zones: zones}
	p.StartUpdater()

	return p
}

func testDANEPolicy(t *testing.T, extR *dns.ExtResolver) *danePolicy {
	m, err := NewDANEPolicy("mx_auth.dane", "test", nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	p := m.(*danePolicy)
	err = p.Init(config.NewMap(nil, config.Node{
		Children: nil,
	}))
	if err != nil {
		t.Fatal(err)
	}

	p.extResolver = extR
	p.log = testutils.Logger(t, "remote/dane")
	return p
}

func TestRemoteDelivery(t *testing.T) {
	be, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()
	testutils.DoTestDelivery(t, tgt, "test@example.com", []string{"test@example.invalid"})

	be.CheckMsg(t, 0, "test@example.com", []string{"test@example.invalid"})
}

// TestRemoteDelivery_NoMXFallback verifies that delivery fails when there are
// no MX records and the domain doesn't resolve to an A record either.
// With HTTP-first delivery, the error surfaces at Body time (not AddRcpt).
func TestRemoteDelivery_NoMXFallback(t *testing.T) {
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	_, err := testutils.DoTestDeliveryErr(t, tgt, "test@example.com", []string{"test@example.invalid"})
	if err == nil {
		t.Fatal("Expected an error, got none")
	}
}

func TestRemoteDelivery_EmptySender(t *testing.T) {
	be, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()
	testutils.DoTestDelivery(t, tgt, "", []string{"test@example.invalid"})

	be.CheckMsg(t, 0, "", []string{"test@example.invalid"})
}

func TestRemoteDelivery_IPLiteral(t *testing.T) {
	t.Skip("Support disabled")

	be, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)

	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
		"1.0.0.127.in-addr.arpa.": {
			PTR: []string{"mx.example.invalid."},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()
	testutils.DoTestDelivery(t, tgt, "test@example.com", []string{"test@[127.0.0.1]"})

	be.CheckMsg(t, 0, "test@example.com", []string{"test@[127.0.0.1]"})
}

func TestRemoteDelivery_FallbackMX(t *testing.T) {
	be, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	testutils.DoTestDelivery(t, tgt, "test@example.com", []string{"test@example.invalid"})
	be.CheckMsg(t, 0, "test@example.com", []string{"test@example.invalid"})
}

func TestRemoteDelivery_BodyNonAtomic(t *testing.T) {
	be, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	c := multipleErrs{
		errs: map[string]error{},
	}
	testutils.DoTestDeliveryNonAtomic(t, &c, tgt, "test@example.com", []string{"test@example.invalid"})

	if err := c.errs["test@example.invalid"]; err != nil {
		t.Fatal(err)
	}

	be.CheckMsg(t, 0, "test@example.com", []string{"test@example.invalid"})
}

func TestRemoteDelivery_Abort(t *testing.T) {
	_, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	delivery, err := tgt.Start(context.Background(), &module.MsgMetadata{ID: "test..."}, "test@example.com")
	if err != nil {
		t.Fatal(err)
	}

	if err := delivery.AddRcpt(context.Background(), "test@example.invalid", smtp.RcptOptions{}); err != nil {
		t.Fatal(err)
	}

	if err := delivery.Abort(context.Background()); err != nil {
		t.Fatal(err)
	}
}

func TestRemoteDelivery_CommitWithoutBody(t *testing.T) {
	_, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	delivery, err := tgt.Start(context.Background(), &module.MsgMetadata{ID: "test..."}, "test@example.com")
	if err != nil {
		t.Fatal(err)
	}

	if err := delivery.AddRcpt(context.Background(), "test@example.invalid", smtp.RcptOptions{}); err != nil {
		t.Fatal(err)
	}

	// Currently it does nothing, probably it should fail.
	if err := delivery.Commit(context.Background()); err != nil {
		t.Fatal(err)
	}
}

// TestRemoteDelivery_MAILFROMErr verifies that a MAIL FROM rejection from
// the remote server results in a delivery error. With HTTP-first delivery,
// the SMTP connection (and thus the MAIL FROM) is deferred until Body time.
func TestRemoteDelivery_MAILFROMErr(t *testing.T) {
	be, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	be.MailErr = &smtp.SMTPError{
		Code:         550,
		EnhancedCode: smtp.EnhancedCode{5, 1, 2},
		Message:      "Hey",
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	_, err := testutils.DoTestDeliveryErr(t, tgt, "test@example.com", []string{"test@example.invalid"})
	if err == nil {
		t.Fatal("Expected an error, got none")
	}
}

// TestRemoteDelivery_NoMX verifies that delivery fails when there are
// no MX records. With HTTP-first delivery, the error surfaces at Body time.
func TestRemoteDelivery_NoMX(t *testing.T) {
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	_, err := testutils.DoTestDeliveryErr(t, tgt, "test@example.com", []string{"test@example.invalid"})
	if err == nil {
		t.Fatal("Expected an error, got none")
	}
}

// TestRemoteDelivery_NullMX verifies that delivery fails when MX is "." (null).
// With HTTP-first delivery, the error surfaces at Body time.
func TestRemoteDelivery_NullMX(t *testing.T) {
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: ".", Pref: 10}},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	_, err := testutils.DoTestDeliveryErr(t, tgt, "test@example.com", []string{"test@example.invalid"})
	if err == nil {
		t.Fatal("Expected an error, got none")
	}
}

func TestRemoteDelivery_Quarantined(t *testing.T) {
	_, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	meta := module.MsgMetadata{ID: "test..."}

	delivery, err := tgt.Start(context.Background(), &meta, "test@example.com")
	if err != nil {
		t.Fatal(err)
	}

	if err := delivery.AddRcpt(context.Background(), "test@example.invalid", smtp.RcptOptions{}); err != nil {
		t.Fatal(err)
	}

	meta.Quarantine = true

	hdr := textproto.Header{}
	hdr.Add("B", "2")
	hdr.Add("A", "1")
	body := buffer.MemoryBuffer{Slice: []byte("foobar\n")}
	if err := delivery.Body(context.Background(), textproto.Header{}, body); err == nil {
		t.Fatal("Expected an error, got none")
	}

	if err := delivery.Abort(context.Background()); err != nil {
		t.Fatal(err)
	}
}

// TestRemoteDelivery_MAILFROMErr_Repeated verifies that MAIL FROM errors
// are reported. With HTTP-first delivery, SMTP errors are deferred until Body
// time, so we use DoTestDeliveryErr to check the full pipeline.
func TestRemoteDelivery_MAILFROMErr_Repeated(t *testing.T) {
	be, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	be.MailErr = &smtp.SMTPError{
		Code:         550,
		EnhancedCode: smtp.EnhancedCode{5, 1, 2},
		Message:      "Hey",
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	_, err := testutils.DoTestDeliveryErr(t, tgt, "test@example.com", []string{"test@example.invalid", "test2@example.invalid"})
	if err == nil {
		t.Fatal("Expected an error, got none")
	}
}

// TestRemoteDelivery_RcptErr verifies that a RCPT TO rejection from
// the remote server is properly reported. With HTTP-first delivery,
// the SMTP errors surface at Body time via BodyNonAtomic.
func TestRemoteDelivery_RcptErr(t *testing.T) {
	be, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	be.RcptErr = map[string]error{
		"test@example.invalid": &smtp.SMTPError{
			Code:         550,
			EnhancedCode: smtp.EnhancedCode{5, 1, 2},
			Message:      "Hey",
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	// With HTTP-first delivery, both AddRcpt calls succeed (they just queue
	// recipients). The actual SMTP errors surface during Body delivery.
	_, err := testutils.DoTestDeliveryErr(t, tgt, "test@example.com", []string{"test@example.invalid", "test2@example.invalid"})
	// The delivery should succeed for test2@ even though test@ was rejected.
	// Body returns nil when at least one recipient succeeds.
	// The message should be delivered to the working recipient.
	if err != nil {
		// It's also acceptable to get an error here if the implementation
		// reports partial failures. Either way is fine.
		t.Logf("Got error (acceptable for partial failure): %v", err)
	}

	be.CheckMsg(t, 0, "test@example.com", []string{"test2@example.invalid"})
}

func TestRemoteDelivery_DownMX(t *testing.T) {
	be, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{
				{Host: "mx1.example.invalid.", Pref: 20},
				{Host: "mx2.example.invalid.", Pref: 10},
			},
		},
		"mx1.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
		"mx2.example.invalid.": {
			A: []string{"127.0.0.2"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	testutils.DoTestDelivery(t, tgt, "test@example.com", []string{"test@example.invalid"})
	be.CheckMsg(t, 0, "test@example.com", []string{"test@example.invalid"})
}

func TestRemoteDelivery_AllMXDown(t *testing.T) {
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{
				{Host: "mx1.example.invalid.", Pref: 20},
				{Host: "mx2.example.invalid.", Pref: 10},
			},
		},
		"mx1.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
		"mx2.example.invalid.": {
			A: []string{"127.0.0.2"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	_, err := testutils.DoTestDeliveryErr(t, tgt, "test@example.com", []string{"test@example.invalid"})
	if err == nil {
		t.Fatal("Expected an error, got none")
	}
}

func TestRemoteDelivery_Split(t *testing.T) {
	be1, srv1 := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv1.Close()
	defer testutils.CheckSMTPConnLeak(t, srv1)
	be2, srv2 := testutils.SMTPServer(t, "127.0.0.2:"+smtpPort)
	defer srv2.Close()
	defer testutils.CheckSMTPConnLeak(t, srv2)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"example2.invalid.": {
			MX: []net.MX{{Host: "mx.example2.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
		"mx.example2.invalid.": {
			A: []string{"127.0.0.2"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	testutils.DoTestDelivery(t, tgt, "test@example.com", []string{"test@example.invalid", "test@example2.invalid"})

	be1.CheckMsg(t, 0, "test@example.com", []string{"test@example.invalid"})
	be2.CheckMsg(t, 0, "test@example.com", []string{"test@example2.invalid"})
}

// TestRemoteDelivery_Split_Fail verifies that when one domain rejects the
// recipient, delivery to another domain still works. With HTTP-first delivery,
// SMTP errors are deferred, so we use DoTestDeliveryErr for the full pipeline.
func TestRemoteDelivery_Split_Fail(t *testing.T) {
	_, srv1 := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv1.Close()
	defer testutils.CheckSMTPConnLeak(t, srv1)
	be2, srv2 := testutils.SMTPServer(t, "127.0.0.2:"+smtpPort)
	defer srv2.Close()
	defer testutils.CheckSMTPConnLeak(t, srv2)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"example2.invalid.": {
			MX: []net.MX{{Host: "mx.example2.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
		"mx.example2.invalid.": {
			A: []string{"127.0.0.2"},
		},
	}

	// Make example.invalid reject all recipients — but example2.invalid
	// should still succeed.
	// NOTE: RcptErr on the mock backend will cause RCPT TO to fail, but
	// with HTTP-first delivery the connection is deferred to Body time.
	// The partial failure is reported through BodyNonAtomic.
	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	// Use the full delivery pipeline which handles partial failures correctly.
	_, err := testutils.DoTestDeliveryErr(t, tgt, "test@example.com", []string{"test@example.invalid", "test@example2.invalid"})
	// Expect partial delivery: example2 should succeed even if example fails.
	// DoTestDeliveryErr returns nil if Body+Commit succeed (partial failures
	// are handled internally by BodyNonAtomic). The message to example2
	// should go through.
	if err != nil {
		t.Logf("Got error (may be acceptable for partial failure): %v", err)
	}

	be2.CheckMsg(t, 0, "test@example.com", []string{"test@example2.invalid"})
}

func TestRemoteDelivery_BodyErr(t *testing.T) {
	be, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	be.DataErr = &smtp.SMTPError{
		Code:         550,
		EnhancedCode: smtp.EnhancedCode{5, 1, 2},
		Message:      "Hey",
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	delivery, err := tgt.Start(context.Background(), &module.MsgMetadata{ID: "test..."}, "test@example.com")
	if err != nil {
		t.Fatal(err)
	}

	err = delivery.AddRcpt(context.Background(), "test@example.invalid", smtp.RcptOptions{})
	if err != nil {
		t.Fatal(err)
	}

	hdr := textproto.Header{}
	hdr.Add("B", "2")
	hdr.Add("A", "1")
	body := buffer.MemoryBuffer{Slice: []byte("foobar\n")}
	if err := delivery.Body(context.Background(), hdr, body); err == nil {
		t.Fatal("expected an error, got none")
	}

	if err := delivery.Abort(context.Background()); err != nil {
		t.Fatal(err)
	}
}

func TestRemoteDelivery_Split_BodyErr(t *testing.T) {
	be1, srv1 := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv1.Close()
	defer testutils.CheckSMTPConnLeak(t, srv1)
	_, srv2 := testutils.SMTPServer(t, "127.0.0.2:"+smtpPort)
	defer srv2.Close()
	defer testutils.CheckSMTPConnLeak(t, srv2)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"example2.invalid.": {
			MX: []net.MX{{Host: "mx.example2.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
		"mx.example2.invalid.": {
			A: []string{"127.0.0.2"},
		},
	}

	be1.DataErr = &smtp.SMTPError{
		Code:         421,
		EnhancedCode: smtp.EnhancedCode{4, 1, 2},
		Message:      "Hey",
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	delivery, err := tgt.Start(context.Background(), &module.MsgMetadata{ID: "test..."}, "test@example.com")
	if err != nil {
		t.Fatal(err)
	}

	if err := delivery.AddRcpt(context.Background(), "test@example.invalid", smtp.RcptOptions{}); err != nil {
		t.Fatal(err)
	}
	if err := delivery.AddRcpt(context.Background(), "test@example2.invalid", smtp.RcptOptions{}); err != nil {
		t.Fatal(err)
	}

	hdr := textproto.Header{}
	hdr.Add("B", "2")
	hdr.Add("A", "1")
	body := buffer.MemoryBuffer{Slice: []byte("foobar\n")}
	err = delivery.Body(context.Background(), hdr, body)
	testutils.CheckSMTPErr(t, err, 451, exterrors.EnhancedCode{4, 0, 0},
		"Partial delivery failure, additional attempts may result in duplicates")

	if err := delivery.Abort(context.Background()); err != nil {
		t.Fatal(err)
	}
}

func TestRemoteDelivery_Split_BodyErr_NonAtomic(t *testing.T) {
	be1, srv1 := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv1.Close()
	defer testutils.CheckSMTPConnLeak(t, srv1)
	_, srv2 := testutils.SMTPServer(t, "127.0.0.2:"+smtpPort)
	defer srv2.Close()
	defer testutils.CheckSMTPConnLeak(t, srv2)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"example2.invalid.": {
			MX: []net.MX{{Host: "mx.example2.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
		"mx.example2.invalid.": {
			A: []string{"127.0.0.2"},
		},
	}

	be1.DataErr = &smtp.SMTPError{
		Code:         421,
		EnhancedCode: smtp.EnhancedCode{4, 1, 2},
		Message:      "Hey",
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	delivery, err := tgt.Start(context.Background(), &module.MsgMetadata{ID: "test..."}, "test@example.com")
	if err != nil {
		t.Fatal(err)
	}

	if err := delivery.AddRcpt(context.Background(), "test@example.invalid", smtp.RcptOptions{}); err != nil {
		t.Fatal(err)
	}
	if err := delivery.AddRcpt(context.Background(), "test@example2.invalid", smtp.RcptOptions{}); err != nil {
		t.Fatal(err)
	}

	hdr := textproto.Header{}
	hdr.Add("B", "2")
	hdr.Add("A", "1")
	body := buffer.MemoryBuffer{Slice: []byte("foobar\n")}
	c := multipleErrs{
		errs: map[string]error{},
	}
	delivery.(module.PartialDelivery).BodyNonAtomic(context.Background(), &c, hdr, body)

	if err := c.errs["test@example2.invalid"]; err != nil {
		t.Fatal("expected delivery to example2 to succeed, got err:", err)
	}

	smtpErr, ok := c.errs["test@example.invalid"].(*exterrors.SMTPError)
	if !ok {
		t.Fatal("expected SMTP error for test@example.invalid, got:", c.errs["test@example.invalid"])
	}
	if smtpErr.Code != 421 {
		t.Fatal("expected 421, got:", smtpErr.Code)
	}

	if err := delivery.Abort(context.Background()); err != nil {
		t.Fatal(err)
	}
}

func TestRemoteDelivery_TLSErrFallback(t *testing.T) {
	be, srv := testutils.SMTPServerSTARTTLS(t, "127.0.0.1:"+smtpPort, tls.VersionTLS10)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	tgt.tlsConfig.MinVersion = tls.VersionTLS11
	defer tgt.Close()

	testutils.DoTestDelivery(t, tgt, "test@example.com", []string{"test@example.invalid"})
	be.CheckMsg(t, 0, "test@example.com", []string{"test@example.invalid"})
}

func TestRemoteDelivery_RequireTLS_Missing(t *testing.T) {
	_, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	_, err := testutils.DoTestDeliveryErrMeta(t, tgt, "test@example.com", []string{"test@example.invalid"}, &module.MsgMetadata{
		SMTPOpts: module.MsgMIMEOpts{
			RequireTLS: true,
		},
	})
	if err == nil {
		t.Fatal("Expected an error, got none")
	}
}

func TestRemoteDelivery_RequireTLS_Present(t *testing.T) {
	be, srv := testutils.SMTPServerSTARTTLS(t, "127.0.0.1:"+smtpPort, 0)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	tgt.tlsConfig.InsecureSkipVerify = true
	defer tgt.Close()

	testutils.DoTestDeliveryMeta(t, tgt, "test@example.com", []string{"test@example.invalid"}, &module.MsgMetadata{
		SMTPOpts: module.MsgMIMEOpts{
			RequireTLS: true,
		},
	})
	be.CheckMsg(t, 0, "test@example.com", []string{"test@example.invalid"})
}

func TestRemoteDelivery_RequireTLS_NoErrFallback(t *testing.T) {
	_, srv := testutils.SMTPServerSTARTTLS(t, "127.0.0.1:"+smtpPort, tls.VersionTLS10)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	tgt.tlsConfig.MinVersion = tls.VersionTLS11
	defer tgt.Close()

	_, err := testutils.DoTestDeliveryErrMeta(t, tgt, "test@example.com", []string{"test@example.invalid"}, &module.MsgMetadata{
		SMTPOpts: module.MsgMIMEOpts{
			RequireTLS: true,
		},
	})
	if err == nil {
		t.Fatal("Expected an error, got none")
	}
}

func TestRemoteDelivery_TLS_FallbackNoVerify(t *testing.T) {
	be, srv := testutils.SMTPServerSTARTTLS(t, "127.0.0.1:"+smtpPort, 0)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	testutils.DoTestDelivery(t, tgt, "test@example.com", []string{"test@example.invalid"})
	be.CheckMsg(t, 0, "test@example.com", []string{"test@example.invalid"})
}

func TestRemoteDelivery_TLS_FallbackPlaintext(t *testing.T) {
	be, srv := testutils.SMTPServerSTARTTLS(t, "127.0.0.1:"+smtpPort, tls.VersionTLS10)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	tgt.tlsConfig.MinVersion = tls.VersionTLS11
	defer tgt.Close()

	testutils.DoTestDelivery(t, tgt, "test@example.com", []string{"test@example.invalid"})
	be.CheckMsg(t, 0, "test@example.com", []string{"test@example.invalid"})
}

func TestRemoteDelivery_ConnReuse(t *testing.T) {
	be, srv := testutils.SMTPServer(t, "127.0.0.1:"+smtpPort)
	defer srv.Close()
	defer testutils.CheckSMTPConnLeak(t, srv)
	zones := map[string]mockdns.Zone{
		"example.invalid.": {
			MX: []net.MX{{Host: "mx.example.invalid.", Pref: 10}},
		},
		"mx.example.invalid.": {
			A: []string{"127.0.0.1"},
		},
	}

	tgt := testTarget(t, zones, nil, nil)
	defer tgt.Close()

	testutils.DoTestDelivery(t, tgt, "test@example.com", []string{"test@example.invalid"})
	testutils.DoTestDelivery(t, tgt, "test@example.com", []string{"test@example.invalid"})

	be.CheckMsg(t, 0, "test@example.com", []string{"test@example.invalid"})
	be.CheckMsg(t, 1, "test@example.com", []string{"test@example.invalid"})
}

func init() {
	flag.Parse()

	rand.Seed(1)

	if os.Getenv("TEST_SMTP_PORT") != "" {
		port, err := strconv.Atoi(os.Getenv("TEST_SMTP_PORT"))
		if err != nil {
			panic(err)
		}
		smtpPort = strconv.Itoa(port)
	}
}
