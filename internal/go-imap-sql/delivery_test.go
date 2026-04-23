package imapsql

import (
	"bufio"
	"io/ioutil"
	"strings"
	"testing"

	"github.com/emersion/go-imap"
	"github.com/emersion/go-imap/backend"
	"github.com/emersion/go-message/textproto"
	"gotest.tools/assert"
	is "gotest.tools/assert/cmp"
)

var testMsgFetchItems = []imap.FetchItem{imap.FetchEnvelope, imap.FetchFlags, imap.FetchBodyStructure, imap.FetchRFC822Size /*"BODY.PEEK[]",*/, "BODY.PEEK[HEADER]", "BODY.PEEK[TEXT]"}

func checkTestMsg(t *testing.T, msg *imap.Message) {
	t.Helper()

	hello := "Hello!"

	for _, item := range msg.Items {
		switch item {
		case imap.FetchEnvelope:
			assert.DeepEqual(t, msg.Envelope, &imap.Envelope{
				Subject: hello,
				From: []*imap.Address{
					{
						MailboxName: "foxcpp",
						HostName:    "foxcpp.dev",
					},
				},
			})
		case imap.FetchFlags:
			assert.DeepEqual(t, msg.Flags, []string{imap.RecentFlag})
		case imap.FetchBodyStructure:
			assert.Equal(t, msg.BodyStructure.MIMEType, "text")
			assert.Equal(t, msg.BodyStructure.MIMESubType, "plain")
		case imap.FetchRFC822Size:
			assert.Equal(t, msg.Size, len(testMsg))
		}
	}

	for key, literal := range msg.Body {
		blob, err := ioutil.ReadAll(literal)
		assert.NilError(t, err, "ReadAll literal")
		switch fetchItem := key.FetchItem(); fetchItem {
		case "BODY.PEEK[]":
			assert.DeepEqual(t, string(blob), testMsg)
		case "BODY.PEEK[HEADER]":
			assert.DeepEqual(t, string(blob), testMsgHeader)
		case "BODY.PEEK[TEXT]":
			assert.DeepEqual(t, string(blob), testMsgBody)
		default:
			t.Log("Unknown part:", fetchItem)
		}
	}
}

type noopConn struct{}

func (n *noopConn) SendUpdate(_ backend.Update) error {
	return nil
}

func TestDelivery(t *testing.T) {
	b := initTestBackend().(*Backend)
	defer cleanBackend(b)
	assert.NilError(t, b.CreateUser(t.Name()+"-1"), "CreateUser 1")
	assert.NilError(t, b.CreateUser(t.Name()+"-2"), "CreateUser 2")

	delivery := b.NewDelivery()

	assert.NilError(t, delivery.AddRcpt(t.Name()+"-1", textproto.Header{}), "AddRcpt 1")
	assert.NilError(t, delivery.AddRcpt(t.Name()+"-2", textproto.Header{}), "AddRcpt 2")

	assert.NilError(t, delivery.BodyRaw(strings.NewReader(testMsg)), "BodyRaw")
	assert.NilError(t, delivery.Commit(), "Commit")

	u1, err := b.GetUser(t.Name() + "-1")
	assert.NilError(t, err, "GetUser 1")
	u2, err := b.GetUser(t.Name() + "-2")
	assert.NilError(t, err, "GetUser 2")

	_, mbox1, err := u1.GetMailbox("INBOX", true, &noopConn{})
	assert.NilError(t, err, "GetMailbox 1 INBOX")
	defer mbox1.Close()
	_, mbox2, err := u2.GetMailbox("INBOX", true, &noopConn{})
	assert.NilError(t, err, "GetMailbox 2 INBOX")
	defer mbox2.Close()

	seq, _ := imap.ParseSeqSet("1:*")
	ch := make(chan *imap.Message, 10)

	assert.NilError(t, mbox1.ListMessages(false, seq, testMsgFetchItems, ch), "ListMessages")
	assert.Assert(t, is.Len(ch, 1))
	msg := <-ch
	checkTestMsg(t, msg)

	hasRecent := false
	for _, flag := range msg.Flags {
		if flag == imap.RecentFlag {
			hasRecent = true
		}
	}
	assert.Assert(t, hasRecent)

	ch = make(chan *imap.Message, 10)
	assert.NilError(t, mbox2.ListMessages(false, seq, testMsgFetchItems, ch), "ListMessages")
	assert.Assert(t, is.Len(ch, 1))
	msg = <-ch
	checkTestMsg(t, msg)

	hasRecent = false
	for _, flag := range msg.Flags {
		if flag == imap.RecentFlag {
			hasRecent = true
		}
	}
	assert.Assert(t, hasRecent)
}

func TestDelivery_Abort(t *testing.T) {
	b := initTestBackend().(*Backend)
	defer cleanBackend(b)
	assert.NilError(t, b.CreateUser(t.Name()), "CreateUser")

	delivery := b.NewDelivery()
	assert.NilError(t, delivery.AddRcpt(t.Name(), textproto.Header{}), "AddRcpt")
	assert.NilError(t, delivery.BodyRaw(strings.NewReader(testMsg)), "BodyRaw")
	assert.NilError(t, delivery.Abort(), "Abort")

	u, err := b.GetUser(t.Name())
	assert.NilError(t, err, "GetUser")
	status, mbox, err := u.GetMailbox("INBOX", true, &noopConn{})
	assert.NilError(t, err, "GetMailbox")
	defer mbox.Close()
	assert.Equal(t, status.Messages, uint32(0))
}

func TestDelivery_AddRcpt_NonExistent(t *testing.T) {
	b := initTestBackend().(*Backend)
	defer cleanBackend(b)
	assert.NilError(t, b.CreateUser(t.Name()), "CreateUser")

	delivery := b.NewDelivery()
	assert.NilError(t, delivery.AddRcpt(t.Name(), textproto.Header{}))

	err := delivery.AddRcpt("NON-EXISTENT", textproto.Header{})
	assert.Assert(t, err != nil, "AddRcpt NON-EXISTENT INBOX")

	// Then, however, delivery should continue as if nothing happened.
	assert.NilError(t, delivery.BodyRaw(strings.NewReader(testMsg)), "BodyRaw")
	assert.NilError(t, delivery.Commit(), "Commit")

	// Check whether the message is delivered.
	u, err := b.GetUser(t.Name())
	assert.NilError(t, err, "GetUser 1")
	_, mbox, err := u.GetMailbox("INBOX", true, &noopConn{})
	assert.NilError(t, err, "GetMailbox INBOX")
	defer mbox.Close()

	seq, _ := imap.ParseSeqSet("*")
	ch := make(chan *imap.Message, 10)

	assert.NilError(t, mbox.ListMessages(false, seq, testMsgFetchItems, ch), "ListMessages")
	assert.Assert(t, is.Len(ch, 1))
	msg := <-ch
	checkTestMsg(t, msg)

	// Below is subtest that verifys whether the the entities created later with non-existent names
	// are not suddenly populated with our message.

	t.Run("NON-EXISTENT user created empty", func(t *testing.T) {
		assert.NilError(t, b.CreateUser("NON-EXISTENT"), "CreateUser NON-EXISTENT")
		u, err := b.GetUser("NON-EXISTENT")
		assert.NilError(t, err, "GetUser NON-EXISTENT")
		status, mbox, err := u.GetMailbox("INBOX", true, &noopConn{})
		assert.NilError(t, err, "GetMailbox INBOX")
		defer mbox.Close()

		assert.Equal(t, status.Messages, uint32(0), "INBOX of NON-EXISTENT user is non-empty")
	})
}

func TestDelivery_Mailbox(t *testing.T) {
	test := func(t *testing.T, create bool) {
		b := initTestBackend().(*Backend)
		defer cleanBackend(b)
		assert.NilError(t, b.CreateUser(t.Name()), "CreateUser")
		u, err := b.GetUser(t.Name())
		assert.NilError(t, err, "GetUser")
		if create {
			assert.NilError(t, u.CreateMailbox("Box"))
		}

		delivery := b.NewDelivery()

		assert.NilError(t, delivery.AddRcpt(t.Name(), textproto.Header{}), "AddRcpt")

		assert.NilError(t, delivery.Mailbox("Box"))
		assert.NilError(t, delivery.BodyRaw(strings.NewReader(testMsg)), "BodyRaw")
		assert.NilError(t, delivery.Commit(), "Commit")

		_, mbox, err := u.GetMailbox("Box", true, &noopConn{})
		assert.NilError(t, err, "GetMailbox Box")
		defer mbox.Close()

		seq, _ := imap.ParseSeqSet("*")
		ch := make(chan *imap.Message, 10)

		assert.NilError(t, mbox.ListMessages(false, seq, testMsgFetchItems, ch), "ListMessages")
		assert.Assert(t, is.Len(ch, 1))
		msg := <-ch
		checkTestMsg(t, msg)
	}

	test(t, true)
	t.Run("nonexistent", func(t *testing.T) {
		test(t, false)
	})
}

func TestDelivery_SpecialMailbox(t *testing.T) {
	test := func(t *testing.T, create bool, specialUse string) {
		b := initTestBackend().(*Backend)
		defer cleanBackend(b)
		assert.NilError(t, b.CreateUser(t.Name()), "CreateUser")
		u, err := b.GetUser(t.Name())
		assert.NilError(t, err, "GetUser")
		if create {
			assert.NilError(t, u.(*User).CreateMailboxSpecial("Box", specialUse))
		}

		delivery := b.NewDelivery()

		assert.NilError(t, delivery.AddRcpt(t.Name(), textproto.Header{}), "AddRcpt")

		assert.NilError(t, delivery.SpecialMailbox(specialUse, "Box"))
		assert.NilError(t, delivery.BodyRaw(strings.NewReader(testMsg)), "BodyRaw")
		assert.NilError(t, delivery.Commit(), "Commit")

		_, mbox, err := u.GetMailbox("Box", true, &noopConn{})
		assert.NilError(t, err, "GetMailbox Box")
		defer mbox.Close()

		seq, _ := imap.ParseSeqSet("*")
		ch := make(chan *imap.Message, 10)

		assert.NilError(t, mbox.ListMessages(false, seq, testMsgFetchItems, ch), "ListMessages")
		assert.Assert(t, is.Len(ch, 1))
		msg := <-ch
		checkTestMsg(t, msg)

		if create {
			info, err := u.ListMailboxes(false)
			assert.NilError(t, err, "ListMailboxes failed")

			for _, box := range info {
				if box.Name != mbox.Name() {
					continue
				}

				containsSpecial := false
				for _, attr := range box.Attributes {
					if attr == specialUse {
						containsSpecial = true
					}
				}
				assert.Assert(t, containsSpecial, "Missing SPECIAL-USE attr")
			}
		}
	}

	test(t, true, imap.JunkAttr)
	t.Run("nonexistent", func(t *testing.T) {
		test(t, false, imap.JunkAttr)
	})
}

func TestDelivery_BodyParsed(t *testing.T) {
	b := initTestBackend().(*Backend)
	defer cleanBackend(b)
	assert.NilError(t, b.CreateUser(t.Name()), "CreateUser")

	delivery := b.NewDelivery()

	assert.NilError(t, delivery.AddRcpt(t.Name(), textproto.Header{}), "AddRcpt")

	buf := memoryBuffer{slice: []byte(testMsgBody)}
	hdr, _ := textproto.ReadHeader(bufio.NewReader(strings.NewReader(testMsgHeader)))
	assert.NilError(t, delivery.BodyParsed(hdr, len(testMsgBody), buf), "BodyParsed")
	assert.NilError(t, delivery.Commit(), "Commit")

	u, err := b.GetUser(t.Name())
	assert.NilError(t, err, "GetUser")

	_, mbox, err := u.GetMailbox("INBOX", true, &noopConn{})
	assert.NilError(t, err, "GetMailbox INBOX")
	defer mbox.Close()

	seq, _ := imap.ParseSeqSet("*")
	ch := make(chan *imap.Message, 10)

	assert.NilError(t, mbox.ListMessages(false, seq, testMsgFetchItems, ch), "ListMessages")
	assert.Assert(t, is.Len(ch, 1))
	msg := <-ch
	checkTestMsg(t, msg)
}

func TestDelivery_UserHeader(t *testing.T) {
	b := initTestBackend().(*Backend)
	defer cleanBackend(b)
	assert.NilError(t, b.CreateUser(t.Name()+"-1"), "CreateUser 1")
	assert.NilError(t, b.CreateUser(t.Name()+"-2"), "CreateUser 2")

	delivery := b.NewDelivery()

	// With hardlink dedup, per-recipient headers are merged from
	// the first recipient only; the value "1" will appear for both.
	hdr1 := textproto.Header{}
	hdr1.Set("Test-Header", "1")
	assert.NilError(t, delivery.AddRcpt(t.Name()+"-1", hdr1), "AddRcpt 1")
	hdr2 := textproto.Header{}
	hdr2.Set("Test-Header", "2")
	assert.NilError(t, delivery.AddRcpt(t.Name()+"-2", hdr2), "AddRcpt 2")

	assert.NilError(t, delivery.BodyRaw(strings.NewReader(testMsg)), "BodyRaw")
	assert.NilError(t, delivery.Commit(), "Commit")

	u1, err := b.GetUser(t.Name() + "-1")
	assert.NilError(t, err, "GetUser 1")
	u2, err := b.GetUser(t.Name() + "-2")
	assert.NilError(t, err, "GetUser 2")

	_, mbox1, err := u1.GetMailbox("INBOX", true, &noopConn{})
	assert.NilError(t, err, "GetMailbox 1 INBOX")
	defer mbox1.Close()
	_, mbox2, err := u2.GetMailbox("INBOX", true, &noopConn{})
	assert.NilError(t, err, "GetMailbox 2 INBOX")
	defer mbox2.Close()

	seq, _ := imap.ParseSeqSet("*")
	ch := make(chan *imap.Message, 10)

	assert.NilError(t, mbox1.ListMessages(false, seq, []imap.FetchItem{"BODY.PEEK[HEADER]"}, ch), "ListMessages")
	assert.Assert(t, is.Len(ch, 1))
	msg := <-ch
	for _, part := range msg.Body {
		hdr, err := textproto.ReadHeader(bufio.NewReader(part))
		assert.NilError(t, err, "ReadHeader")
		assert.Check(t, is.Equal(hdr.Get("Test-Header"), "1"), "wrong user header stored")
	}

	ch = make(chan *imap.Message, 10)
	assert.NilError(t, mbox2.ListMessages(false, seq, []imap.FetchItem{"BODY.PEEK[HEADER]"}, ch), "ListMessages")
	assert.Assert(t, is.Len(ch, 1))
	msg = <-ch
	for _, part := range msg.Body {
		hdr, err := textproto.ReadHeader(bufio.NewReader(part))
		assert.NilError(t, err, "ReadHeader")
		// Dedup: both recipients share the blob from the first
		// recipient, so this header reflects user-1's merge.
		assert.Check(t, is.Equal(hdr.Get("Test-Header"), "1"), "dedup: all recipients share first blob")
	}
}
