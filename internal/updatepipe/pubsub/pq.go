package pubsub

import (
	"context"
	"sync"
	"time"

	"github.com/lib/pq"
	"github.com/themadorg/madmail/framework/log"
	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"
)

type Msg struct {
	Key     string
	Payload string
}

type PqPubSub struct {
	Notify chan Msg

	mu     sync.Mutex
	L      *pq.Listener
	sender *gorm.DB

	Log log.Logger
}

func NewPQ(dsn string) (*PqPubSub, error) {
	l := &PqPubSub{
		Log: log.Logger{Name: "pgpubsub"},
		// Buffer size increased to handle bursts from many concurrent users
		// With 1000 users and messages to 100 recipients, we need substantial buffering
		Notify: make(chan Msg, 1024),
	}
	l.L = pq.NewListener(dsn, 10*time.Second, time.Minute, l.eventHandler)
	var err error
	l.sender, err = mdb.New("postgres", []string{dsn}, l.Log.Debug)
	if err != nil {
		return nil, err
	}

	go func() {
		defer close(l.Notify)
		for n := range l.L.Notify {
			if n == nil {
				continue
			}

			l.Notify <- Msg{Key: n.Channel, Payload: n.Extra}
		}
	}()

	return l, nil
}

func (l *PqPubSub) Close() error {
	sqlDB, _ := l.sender.DB()
	if sqlDB != nil {
		sqlDB.Close()
	}
	l.L.Close()
	return nil
}

func (l *PqPubSub) eventHandler(ev pq.ListenerEventType, err error) {
	switch ev {
	case pq.ListenerEventConnected:
		l.Log.DebugMsg("connected")
	case pq.ListenerEventReconnected:
		l.Log.Msg("connection reestablished")
	case pq.ListenerEventConnectionAttemptFailed:
		l.Log.Error("connection attempt failed", err)
	case pq.ListenerEventDisconnected:
		l.Log.Msg("connection closed", "err", err)
	}
}

func (l *PqPubSub) Subscribe(_ context.Context, key string) error {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.L.Listen(key)
}

func (l *PqPubSub) Unsubscribe(_ context.Context, key string) error {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.L.Unlisten(key)
}

func (l *PqPubSub) Publish(key, payload string) error {
	return l.sender.Exec(`SELECT pg_notify($1, $2)`, key, payload).Error
}

func (l *PqPubSub) Listener() chan Msg {
	return l.Notify
}
