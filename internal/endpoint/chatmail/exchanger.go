package chatmail

import (
	"bufio"
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"github.com/emersion/go-message/textproto"
	"github.com/emersion/go-smtp"
	"github.com/themadorg/madmail/framework/buffer"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/db"
)

type exchangerResponse struct {
	Hash     string             `json:"hash"`
	Count    int                `json:"count"`
	Messages []exchangerMessage `json:"messages"`
}

type exchangerMessage struct {
	ID   string   `json:"id"`
	From string   `json:"from"`
	To   []string `json:"to"`
	Size int      `json:"size"`
	Date string   `json:"date"`
	Body string   `json:"body"` // Base64
}

func (e *Endpoint) runExchangerPoller() {
	// Wait a bit before starting first poll to allow other modules to initialize
	time.Sleep(10 * time.Second)

	e.logger.Msg("Starting exchanger poller")
	ticker := time.NewTicker(1 * time.Second) // Check every 1 second
	defer ticker.Stop()

	for range ticker.C {
		e.pollExchangers()
	}
}

func (e *Endpoint) pollExchangers() {
	if e.exchangerGORM == nil {
		return
	}

	var exchangers []db.Exchanger
	if err := e.exchangerGORM.Where("enabled = ?", true).Find(&exchangers).Error; err != nil {
		return
	}

	e.logger.Msg(fmt.Sprintf("[exchanger] Found %d exchangers to poll", len(exchangers)))

	for _, ex := range exchangers {
		// Respect per-exchanger polling interval
		interval := 60 // default
		if ex.PollInterval > 0 {
			interval = ex.PollInterval
		}

		if !ex.LastPollAt.IsZero() && time.Since(ex.LastPollAt) < time.Duration(interval)*time.Second {
			continue
		}

		// Only pull for OURSELVES.
		if err := e.pollOne(ex, e.mailDomain); err != nil {
			e.logger.Error(fmt.Sprintf("failed to poll exchanger %s at %s for ourselves (%s)", ex.Name, ex.URL, e.mailDomain), err)
		}
	}
}

func (e *Endpoint) pollOne(ex db.Exchanger, domain string) error {
	// Update last poll time immediately so the interval check works correctly
	// even when there are zero messages (prevents re-polling every tick).
	e.exchangerGORM.Model(&ex).Update("last_poll_at", time.Now())

	// The exchanger now uses a unified queue called 'me'
	url := fmt.Sprintf("%s/me/full", strings.TrimSuffix(ex.URL, "/"))

	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()

	req, err := http.NewRequestWithContext(ctx, "GET", url, nil)
	if err != nil {
		return err
	}

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		if resp.StatusCode == http.StatusNotFound {
			return nil // No queue yet or no messages
		}
		return fmt.Errorf("HTTP error: %s", resp.Status)
	}

	var res exchangerResponse
	if err := json.NewDecoder(resp.Body).Decode(&res); err != nil {
		return err
	}

	if res.Count == 0 {
		return nil
	}

	e.logger.Msg(fmt.Sprintf("[exchanger] downloading %d messages from %s for %s", res.Count, ex.Name, domain))

	// 3. Process messages
	for _, msg := range res.Messages {
		if err := e.injectMessage(msg); err != nil {
			e.logger.Error(fmt.Sprintf("failed to inject message %s", msg.ID), err)
		} else {
			e.logger.Msg(fmt.Sprintf("[exchanger] message %s injected successfully", msg.ID))
		}
	}

	return nil
}

func (e *Endpoint) injectMessage(msg exchangerMessage) error {
	body, err := base64.StdEncoding.DecodeString(msg.Body)
	if err != nil {
		return fmt.Errorf("base64 decode failed: %w", err)
	}

	dt, ok := e.storage.(module.DeliveryTarget)
	if !ok {
		return fmt.Errorf("storage does not implement DeliveryTarget")
	}

	ctx := context.Background() // Use background for delivery
	msgID, _ := module.GenerateMsgID()
	msgMeta := &module.MsgMetadata{
		ID:       msgID,
		SMTPOpts: smtp.MailOptions{},
	}

	delivery, err := dt.Start(ctx, msgMeta, msg.From)
	if err != nil {
		return err
	}
	defer func() { _ = delivery.Abort(ctx) }()

	anyAccepted := false
	for _, to := range msg.To {
		if err := delivery.AddRcpt(ctx, to, smtp.RcptOptions{}); err != nil {
			e.logger.Error("failed to add recipient during pull", err, "to", to)
		} else {
			anyAccepted = true
		}
	}

	if !anyAccepted {
		return fmt.Errorf("no valid recipients accepted for message %s", msg.ID)
	}

	br := bufio.NewReader(bytes.NewReader(body))
	header, err := textproto.ReadHeader(br)
	if err != nil {
		return fmt.Errorf("failed to parse pulled message header: %w", err)
	}

	remainingBody, _ := io.ReadAll(br)
	b := buffer.MemoryBuffer{Slice: remainingBody}

	if err := delivery.Body(ctx, header, b); err != nil {
		return err
	}

	if err := delivery.Commit(ctx); err != nil {
		return err
	}

	e.logger.Msg("[exchanger] pulled message delivered", "from", msg.From, "to", msg.To, "id", msg.ID)
	module.IncrementReceivedMessages()
	return nil
}
