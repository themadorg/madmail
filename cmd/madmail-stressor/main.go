package main

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"math"
	"net"
	"os"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

type Config struct {
	TargetAddr        string
	HeloDomain        string
	MailFrom          string
	RcptTo            string
	Duration          time.Duration
	Concurrency       int
	MessagesPerWorker int
	MessageBytes      int
	ConnectTimeout    time.Duration
	IOTimeout         time.Duration
	MaxLatencySamples int
}

type StageResult struct {
	Workers         int               `json:"workers"`
	DurationSeconds float64           `json:"duration_seconds"`
	Attempts        uint64            `json:"attempts"`
	Successes       uint64            `json:"successes"`
	Failures        uint64            `json:"failures"`
	SuccessRatePct  float64           `json:"success_rate_pct"`
	ThroughputMPS   float64           `json:"throughput_mps"`
	AvgLatencyMS    float64           `json:"avg_latency_ms"`
	P50LatencyMS    float64           `json:"p50_latency_ms"`
	P95LatencyMS    float64           `json:"p95_latency_ms"`
	P99LatencyMS    float64           `json:"p99_latency_ms"`
	MinLatencyMS    float64           `json:"min_latency_ms"`
	MaxLatencyMS    float64           `json:"max_latency_ms"`
	Errors          map[string]uint64 `json:"errors,omitempty"`
}

type Report struct {
	GeneratedAt       string        `json:"generated_at"`
	Target            string        `json:"target"`
	HeloDomain        string        `json:"helo_domain"`
	MailFrom          string        `json:"mail_from"`
	RcptTo            string        `json:"rcpt_to"`
	DurationSeconds   float64       `json:"duration_seconds"`
	MessageBytes      int           `json:"message_bytes"`
	MessagesPerWorker int           `json:"messages_per_worker"`
	Stages            []StageResult `json:"stages"`
}

type metrics struct {
	attempts  atomic.Uint64
	successes atomic.Uint64
	failures  atomic.Uint64

	latMu     sync.Mutex
	latencies []time.Duration
	maxLats   int

	errMu  sync.Mutex
	errors map[string]uint64
}

func newMetrics(maxLatencySamples int) *metrics {
	if maxLatencySamples <= 0 {
		maxLatencySamples = 100000
	}
	return &metrics{
		latencies: make([]time.Duration, 0, min(maxLatencySamples, 1024)),
		maxLats:   maxLatencySamples,
		errors:    make(map[string]uint64),
	}
}

func (m *metrics) recordLatency(d time.Duration) {
	m.latMu.Lock()
	if len(m.latencies) < m.maxLats {
		m.latencies = append(m.latencies, d)
	}
	m.latMu.Unlock()
}

func (m *metrics) snapshotLatencies() []time.Duration {
	m.latMu.Lock()
	defer m.latMu.Unlock()
	cp := make([]time.Duration, len(m.latencies))
	copy(cp, m.latencies)
	return cp
}

func (m *metrics) recordError(key string) {
	m.errMu.Lock()
	m.errors[key]++
	m.errMu.Unlock()
}

func (m *metrics) snapshotErrors() map[string]uint64 {
	m.errMu.Lock()
	defer m.errMu.Unlock()
	cp := make(map[string]uint64, len(m.errors))
	for k, v := range m.errors {
		cp[k] = v
	}
	return cp
}

type smtpStatusError struct {
	Code    int
	Message string
}

func (e *smtpStatusError) Error() string {
	if e.Message == "" {
		return fmt.Sprintf("smtp status %d", e.Code)
	}
	return fmt.Sprintf("smtp status %d: %s", e.Code, e.Message)
}

func categorizeError(err error) string {
	var statusErr *smtpStatusError
	if errors.As(err, &statusErr) {
		return fmt.Sprintf("smtp_%d", statusErr.Code)
	}

	var netErr net.Error
	if errors.As(err, &netErr) && netErr.Timeout() {
		return "timeout"
	}

	if errors.Is(err, ioEOF) {
		return "eof"
	}
	return "io_error"
}

var ioEOF = errors.New("io eof")

func parseRamp(raw string) ([]int, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil, nil
	}

	parts := strings.Split(raw, ",")
	vals := make([]int, 0, len(parts))
	for _, part := range parts {
		part = strings.TrimSpace(part)
		if part == "" {
			continue
		}
		v, err := strconv.Atoi(part)
		if err != nil {
			return nil, fmt.Errorf("invalid ramp value %q: %w", part, err)
		}
		if v <= 0 {
			return nil, fmt.Errorf("ramp values must be > 0, got %d", v)
		}
		vals = append(vals, v)
	}
	if len(vals) == 0 {
		return nil, errors.New("ramp has no valid values")
	}
	return vals, nil
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func percentileMS(sorted []float64, p float64) float64 {
	if len(sorted) == 0 {
		return 0
	}
	if p <= 0 {
		return sorted[0]
	}
	if p >= 100 {
		return sorted[len(sorted)-1]
	}
	rank := (p / 100.0) * float64(len(sorted)-1)
	lower := int(math.Floor(rank))
	upper := int(math.Ceil(rank))
	if lower == upper {
		return sorted[lower]
	}
	frac := rank - float64(lower)
	return sorted[lower] + frac*(sorted[upper]-sorted[lower])
}

func computeLatencyStats(latencies []time.Duration) (avg, p50, p95, p99, minV, maxV float64) {
	if len(latencies) == 0 {
		return 0, 0, 0, 0, 0, 0
	}
	values := make([]float64, len(latencies))
	total := 0.0
	for i, lat := range latencies {
		ms := float64(lat) / float64(time.Millisecond)
		values[i] = ms
		total += ms
	}
	sort.Float64s(values)
	avg = total / float64(len(values))
	p50 = percentileMS(values, 50)
	p95 = percentileMS(values, 95)
	p99 = percentileMS(values, 99)
	minV = values[0]
	maxV = values[len(values)-1]
	return
}

func worker(ctx context.Context, cfg Config, m *metrics, workerID int) {
	attempts := 0
	for {
		if cfg.MessagesPerWorker > 0 && attempts >= cfg.MessagesPerWorker {
			return
		}
		select {
		case <-ctx.Done():
			return
		default:
		}

		attempts++
		m.attempts.Add(1)
		start := time.Now()
		err := sendSMTPMessage(ctx, cfg, workerID, attempts)
		if err != nil {
			m.failures.Add(1)
			m.recordError(categorizeError(err))
			continue
		}

		m.successes.Add(1)
		m.recordLatency(time.Since(start))
	}
}

func runStage(parent context.Context, cfg Config, workers int) StageResult {
	workers = max(1, workers)
	ctx := parent
	cancel := func() {}
	if cfg.Duration > 0 {
		ctx, cancel = context.WithTimeout(parent, cfg.Duration)
	}
	defer cancel()

	start := time.Now()
	m := newMetrics(cfg.MaxLatencySamples)

	var wg sync.WaitGroup
	wg.Add(workers)
	for i := 0; i < workers; i++ {
		go func(workerID int) {
			defer wg.Done()
			worker(ctx, cfg, m, workerID)
		}(i + 1)
	}
	wg.Wait()

	elapsed := time.Since(start)
	if elapsed <= 0 {
		elapsed = time.Millisecond
	}

	attempts := m.attempts.Load()
	successes := m.successes.Load()
	failures := m.failures.Load()
	successRate := 0.0
	if attempts > 0 {
		successRate = (float64(successes) / float64(attempts)) * 100
	}
	avg, p50, p95, p99, minV, maxV := computeLatencyStats(m.snapshotLatencies())

	return StageResult{
		Workers:         workers,
		DurationSeconds: elapsed.Seconds(),
		Attempts:        attempts,
		Successes:       successes,
		Failures:        failures,
		SuccessRatePct:  successRate,
		ThroughputMPS:   float64(successes) / elapsed.Seconds(),
		AvgLatencyMS:    avg,
		P50LatencyMS:    p50,
		P95LatencyMS:    p95,
		P99LatencyMS:    p99,
		MinLatencyMS:    minV,
		MaxLatencyMS:    maxV,
		Errors:          m.snapshotErrors(),
	}
}

type smtpClient struct {
	conn net.Conn
	r    *bufio.Reader
	w    *bufio.Writer
}

func newSMTPClient(ctx context.Context, cfg Config) (*smtpClient, error) {
	dialer := net.Dialer{Timeout: cfg.ConnectTimeout}
	conn, err := dialer.DialContext(ctx, "tcp", cfg.TargetAddr)
	if err != nil {
		return nil, err
	}
	if cfg.IOTimeout > 0 {
		_ = conn.SetDeadline(time.Now().Add(cfg.IOTimeout))
	}
	return &smtpClient{
		conn: conn,
		r:    bufio.NewReader(conn),
		w:    bufio.NewWriter(conn),
	}, nil
}

func (c *smtpClient) close() {
	_ = c.conn.Close()
}

func readSMTPResponse(r *bufio.Reader) (int, string, error) {
	var code int
	parts := make([]string, 0, 2)
	for {
		line, err := r.ReadString('\n')
		if err != nil {
			if errors.Is(err, os.ErrClosed) {
				return 0, "", ioEOF
			}
			if errors.Is(err, net.ErrClosed) {
				return 0, "", ioEOF
			}
			if strings.Contains(err.Error(), "EOF") {
				return 0, "", ioEOF
			}
			return 0, "", err
		}

		line = strings.TrimRight(line, "\r\n")
		if len(line) < 3 {
			return 0, "", fmt.Errorf("malformed smtp response: %q", line)
		}
		lineCode, err := strconv.Atoi(line[:3])
		if err != nil {
			return 0, "", fmt.Errorf("malformed smtp status code: %q", line)
		}
		if code == 0 {
			code = lineCode
		} else if lineCode != code {
			return 0, "", fmt.Errorf("inconsistent smtp response code: got %d after %d", lineCode, code)
		}

		suffix := ""
		if len(line) > 4 {
			suffix = line[4:]
		}
		parts = append(parts, suffix)

		if len(line) == 3 || line[3] == ' ' {
			break
		}
		if line[3] != '-' {
			return 0, "", fmt.Errorf("malformed smtp continuation: %q", line)
		}
	}

	return code, strings.TrimSpace(strings.Join(parts, "\n")), nil
}

func (c *smtpClient) expect(class int) error {
	code, msg, err := readSMTPResponse(c.r)
	if err != nil {
		return err
	}
	if code/100 != class {
		return &smtpStatusError{Code: code, Message: msg}
	}
	return nil
}

func (c *smtpClient) command(class int, cmd string) error {
	if _, err := c.w.WriteString(cmd + "\r\n"); err != nil {
		return err
	}
	if err := c.w.Flush(); err != nil {
		return err
	}
	return c.expect(class)
}

func (c *smtpClient) sendData(msg string) error {
	if err := c.command(3, "DATA"); err != nil {
		return err
	}
	if _, err := c.w.WriteString(msg); err != nil {
		return err
	}
	if !strings.HasSuffix(msg, "\r\n") {
		if _, err := c.w.WriteString("\r\n"); err != nil {
			return err
		}
	}
	if _, err := c.w.WriteString(".\r\n"); err != nil {
		return err
	}
	if err := c.w.Flush(); err != nil {
		return err
	}
	return c.expect(2)
}

var globalMsgID atomic.Uint64

func buildMessage(cfg Config, workerID, attempt int) string {
	msgID := globalMsgID.Add(1)
	body := "stress"
	if cfg.MessageBytes > 0 {
		body = strings.Repeat("x", cfg.MessageBytes)
	}
	return fmt.Sprintf(
		"From: <%s>\r\nTo: <%s>\r\nSubject: stress-%d-%d\r\nDate: %s\r\nMessage-ID: <%d.%d@%s>\r\n\r\n%s\r\n",
		cfg.MailFrom,
		cfg.RcptTo,
		workerID,
		attempt,
		time.Now().UTC().Format(time.RFC1123Z),
		msgID,
		workerID,
		cfg.HeloDomain,
		body,
	)
}

func sendSMTPMessage(ctx context.Context, cfg Config, workerID, attempt int) error {
	client, err := newSMTPClient(ctx, cfg)
	if err != nil {
		return err
	}
	defer client.close()

	if err := client.expect(2); err != nil {
		return err
	}
	if err := client.command(2, "EHLO "+cfg.HeloDomain); err != nil {
		return err
	}
	if err := client.command(2, "MAIL FROM:<"+cfg.MailFrom+">"); err != nil {
		return err
	}
	if err := client.command(2, "RCPT TO:<"+cfg.RcptTo+">"); err != nil {
		return err
	}
	if err := client.sendData(buildMessage(cfg, workerID, attempt)); err != nil {
		return err
	}
	_ = client.command(2, "QUIT")
	return nil
}

func writeJSON(path string, report Report) error {
	if path == "" {
		return nil
	}
	blob, err := json.MarshalIndent(report, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, blob, 0o644)
}

func writeMarkdown(path string, report Report) error {
	if path == "" {
		return nil
	}
	var b strings.Builder
	b.WriteString("# Madmail Go Stress Report\n\n")
	b.WriteString("## Target\n")
	b.WriteString(fmt.Sprintf("- Address: %s\n", report.Target))
	b.WriteString(fmt.Sprintf("- Sender: %s\n", report.MailFrom))
	b.WriteString(fmt.Sprintf("- Recipient: %s\n", report.RcptTo))
	b.WriteString(fmt.Sprintf("- Stage duration: %.0fs\n", report.DurationSeconds))
	if report.MessagesPerWorker > 0 {
		b.WriteString(fmt.Sprintf("- Messages/worker/stage: %d\n", report.MessagesPerWorker))
	}
	b.WriteString("\n## Stage Results\n\n")
	b.WriteString("| workers | attempts | success | failure | success % | throughput (msg/s) | p95 latency (ms) |\n")
	b.WriteString("|---:|---:|---:|---:|---:|---:|---:|\n")
	for _, stage := range report.Stages {
		b.WriteString(fmt.Sprintf(
			"| %d | %d | %d | %d | %.2f | %.2f | %.2f |\n",
			stage.Workers,
			stage.Attempts,
			stage.Successes,
			stage.Failures,
			stage.SuccessRatePct,
			stage.ThroughputMPS,
			stage.P95LatencyMS,
		))
	}
	return os.WriteFile(path, []byte(b.String()), 0o644)
}

func defaultHeloDomain() string {
	host, err := os.Hostname()
	if err != nil || host == "" {
		return "localhost"
	}
	return host
}

func main() {
	target := flag.String("target", "127.0.0.1:25", "SMTP target address host:port")
	mailFrom := flag.String("mail-from", "loadtest@example.net", "SMTP envelope sender address")
	rcptTo := flag.String("rcpt-to", "sink@example.net", "SMTP envelope recipient address")
	helo := flag.String("helo", defaultHeloDomain(), "SMTP HELO/EHLO domain")
	concurrency := flag.Int("concurrency", max(1, runtime.NumCPU()*2), "worker count if -ramp is not provided")
	ramp := flag.String("ramp", "", "comma-separated worker stages (example: 16,32,64,128)")
	duration := flag.Duration("duration", 30*time.Second, "stage runtime duration")
	messagesPerWorker := flag.Int("messages-per-worker", 0, "fixed message attempts per worker per stage (overrides duration stop condition when > 0)")
	bodyBytes := flag.Int("body-bytes", 256, "message body size in bytes")
	connectTimeout := flag.Duration("connect-timeout", 3*time.Second, "TCP connect timeout")
	ioTimeout := flag.Duration("io-timeout", 15*time.Second, "SMTP I/O deadline per connection")
	maxLatencySamples := flag.Int("max-latency-samples", 200000, "max in-memory successful latency samples per stage")
	reportJSON := flag.String("report-json", "", "optional JSON report output path")
	reportMD := flag.String("report-md", "", "optional Markdown report output path")
	flag.Parse()

	if *messagesPerWorker <= 0 && *duration <= 0 {
		fmt.Fprintln(os.Stderr, "duration must be > 0 when messages-per-worker is 0")
		os.Exit(2)
	}

	cfg := Config{
		TargetAddr:        *target,
		HeloDomain:        *helo,
		MailFrom:          *mailFrom,
		RcptTo:            *rcptTo,
		Duration:          *duration,
		Concurrency:       max(1, *concurrency),
		MessagesPerWorker: *messagesPerWorker,
		MessageBytes:      max(0, *bodyBytes),
		ConnectTimeout:    *connectTimeout,
		IOTimeout:         *ioTimeout,
		MaxLatencySamples: *maxLatencySamples,
	}

	stages, err := parseRamp(*ramp)
	if err != nil {
		fmt.Fprintf(os.Stderr, "invalid -ramp: %v\n", err)
		os.Exit(2)
	}
	if len(stages) == 0 {
		stages = []int{cfg.Concurrency}
	}

	report := Report{
		GeneratedAt:       time.Now().UTC().Format(time.RFC3339),
		Target:            cfg.TargetAddr,
		HeloDomain:        cfg.HeloDomain,
		MailFrom:          cfg.MailFrom,
		RcptTo:            cfg.RcptTo,
		DurationSeconds:   cfg.Duration.Seconds(),
		MessageBytes:      cfg.MessageBytes,
		MessagesPerWorker: cfg.MessagesPerWorker,
		Stages:            make([]StageResult, 0, len(stages)),
	}

	for _, workers := range stages {
		fmt.Printf("running stage workers=%d ...\n", workers)
		res := runStage(context.Background(), cfg, workers)
		report.Stages = append(report.Stages, res)
		fmt.Printf(
			"workers=%d attempts=%d success=%d failure=%d success_rate=%.2f%% throughput=%.2f msg/s p95=%.2fms\n",
			res.Workers,
			res.Attempts,
			res.Successes,
			res.Failures,
			res.SuccessRatePct,
			res.ThroughputMPS,
			res.P95LatencyMS,
		)
		if len(res.Errors) > 0 {
			fmt.Println("errors:")
			keys := make([]string, 0, len(res.Errors))
			for k := range res.Errors {
				keys = append(keys, k)
			}
			sort.Strings(keys)
			for _, k := range keys {
				fmt.Printf("  %s: %d\n", k, res.Errors[k])
			}
		}
	}

	if err := writeJSON(*reportJSON, report); err != nil {
		fmt.Fprintf(os.Stderr, "failed to write JSON report: %v\n", err)
		os.Exit(1)
	}
	if err := writeMarkdown(*reportMD, report); err != nil {
		fmt.Fprintf(os.Stderr, "failed to write Markdown report: %v\n", err)
		os.Exit(1)
	}
}
