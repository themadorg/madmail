package imapsql

import (
	"database/sql"
	"encoding/json"
	"errors"
	"net/mail"
	"sort"
	"time"

	"github.com/emersion/go-imap"
	sortthread "github.com/emersion/go-imap-sortthread"
)

type msgKey struct {
	ID           uint32
	ArrivalUnix  int64
	BodyLen      uint32
	CachedHeader map[string][]string
}

func (m *Mailbox) Sort(uid bool, sortCrit []sortthread.SortCriterion, searchCrit *imap.SearchCriteria) ([]uint32, error) {
	m.parent.Opts.Log.Debugln("Sort: SORT", uid, sortCrit, searchCrit)
	msgs, err := m.SearchMessages(true, searchCrit)
	if err != nil {
		return nil, err
	}

	if len(msgs) == 0 {
		if uid {
			return nil, nil
		}
		return nil, errors.New("No messages matched the criteria")
	}

	// IDs in msgs are sorted so this will 'compress' adjacent IDs into ranges.
	seqSet := imap.SeqSet{}
	seqSet.AddNum(msgs...)

	m.parent.Opts.Log.Debugln("Sort: SORT found uids", seqSet)

	// XXX: Split SearchMessages to allow it running in the same transaction.

	resultCount := len(msgs)
	if resultCount > 1000 {
		resultCount = 1000
	}
	sortBuffer := make([]*msgKey, 0, resultCount)

	_, err = m.headerMetaScan(nil, &seqSet, func(k *msgKey) error {
		sortBuffer = append(sortBuffer, k)
		return nil
	})
	if err != nil {
		return nil, errors.New("Internal server error")
	}

	m.parent.Opts.Log.Debugln("Sort: sorting", len(sortBuffer), "messages")

	sort.Slice(sortBuffer, messageCompare(m.parent.Opts.Log, sortBuffer, sortCrit))
	ids := make([]uint32, len(sortBuffer))
	for i, msg := range sortBuffer {
		id := msg.ID
		if !uid {
			var ok bool
			id, ok = m.handle.UidAsSeq(id)
			if !ok {
				continue // Wtf
			}
		}
		ids[i] = id
	}
	return ids, nil
}

func firstHeaderField(all []string) string {
	if len(all) > 0 {
		return all[0]
	}
	return ""
}

func firstAddrFromList(all []string) string {
	list, err := mail.ParseAddressList(firstHeaderField(all))
	if err != nil {
		return ""
	}
	if len(list) == 0 {
		return ""
	}
	return list[0].Address
}

func sentDate(dateHeaders []string, arrivalUnix int64) time.Time {
	t, err := parseMessageDateTime(firstHeaderField(dateHeaders))
	if err != nil {
		return time.Unix(arrivalUnix, 0)
	}
	return t.UTC()
}

func messageCompare(log Logger, buf []*msgKey, sortCrit []sortthread.SortCriterion) func(i, j int) bool {
	return func(i, j int) bool {
		for _, crit := range sortCrit {
			switch crit.Field {
			case "ARRIVAL":
				if buf[i].ArrivalUnix == buf[j].ArrivalUnix {
					continue
				}
				if crit.Reverse {
					return buf[i].ArrivalUnix > buf[j].ArrivalUnix
				} else {
					return buf[i].ArrivalUnix < buf[j].ArrivalUnix
				}
			case "CC":
				iAddr := firstAddrFromList(buf[i].CachedHeader["Cc"])
				jAddr := firstAddrFromList(buf[j].CachedHeader["Cc"])
				if iAddr == jAddr {
					continue
				}
				if crit.Reverse {
					return iAddr > jAddr
				} else {
					return iAddr < jAddr
				}
			case "DATE":
				iDate := sentDate(buf[i].CachedHeader["Date"], buf[i].ArrivalUnix)
				jDate := sentDate(buf[j].CachedHeader["Date"], buf[j].ArrivalUnix)
				if iDate == jDate {
					continue
				}
				if crit.Reverse {
					return iDate.After(jDate)
				} else {
					return iDate.Before(jDate)
				}
			case "FROM":
				iAddr := firstAddrFromList(buf[i].CachedHeader["From"])
				jAddr := firstAddrFromList(buf[j].CachedHeader["From"])
				log.Debugln(iAddr, "vs", jAddr, "=>", iAddr < jAddr)
				if iAddr == jAddr {
					continue
				}
				if crit.Reverse {
					return iAddr > jAddr
				} else {
					return iAddr < jAddr
				}
			case "SIZE":
				if buf[i].BodyLen == buf[j].BodyLen {
					continue
				}
				if crit.Reverse {
					return buf[i].BodyLen > buf[j].BodyLen
				} else {
					return buf[i].BodyLen < buf[j].BodyLen
				}
			case "SUBJECT":
				iSubj, _ := sortthread.GetBaseSubject(firstHeaderField(buf[i].CachedHeader["Subject"]))
				jSubj, _ := sortthread.GetBaseSubject(firstHeaderField(buf[j].CachedHeader["Subject"]))
				if iSubj == jSubj {
					continue
				}
				if crit.Reverse {
					return iSubj > jSubj
				} else {
					return iSubj < jSubj
				}
			case "TO":
				iAddr := firstAddrFromList(buf[i].CachedHeader["To"])
				jAddr := firstAddrFromList(buf[j].CachedHeader["To"])
				if iAddr == jAddr {
					continue
				}
				if crit.Reverse {
					return iAddr > jAddr
				} else {
					return iAddr < jAddr
				}
			}
		}
		return buf[i].ID < buf[j].ID
	}
}

func (b *Backend) SupportedThreadAlgorithms() []sortthread.ThreadAlgorithm {
	return []sortthread.ThreadAlgorithm{sortthread.OrderedSubject}
}

func (m *Mailbox) Thread(uid bool, threading sortthread.ThreadAlgorithm, searchCrit *imap.SearchCriteria) ([]*sortthread.Thread, error) {
	m.parent.Opts.Log.Debugln("Sort: THREAD", uid, threading, searchCrit)
	msgs, err := m.SearchMessages(uid, searchCrit)
	if err != nil {
		return nil, err
	}

	if len(msgs) == 0 {
		return nil, errors.New("No messages matched the criteria")
	}

	// IDs in msgs are sorted so this will 'compress' adjacent IDs into ranges
	// and improve meta-data load performance.
	seqSet := imap.SeqSet{}
	seqSet.AddNum(msgs...)

	// TODO: Split SearchMessages to allow it running in the same transaction.

	if threading != sortthread.OrderedSubject {
		return nil, errors.New("Unsupported threading algorithm")
	}

	return m.orderedSubjThread(nil, uid, &seqSet, len(msgs))
}

func (m *Mailbox) orderedSubjThread(tx *sql.Tx, uid bool, seqSet *imap.SeqSet, msgCount int) ([]*sortthread.Thread, error) {
	type msg struct {
		id       uint32
		sentDate int64
	}
	// Some educated guess for size to reduce amount of reallocations needed for hash map.
	// based on assumption that most messages do not have replies or forwards.
	threads := make(map[string][]msg, msgCount/9*10)

	count, err := m.headerMetaScan(tx, seqSet, func(k *msgKey) error {
		subject, _ := sortthread.GetBaseSubject(firstHeaderField(k.CachedHeader["Subject"]))
		sentDate := sentDate(k.CachedHeader["Date"], k.ArrivalUnix)

		if threads[subject] == nil {
			threads[subject] = []msg{}
		}
		threads[subject] = append(threads[subject], msg{
			id:       k.ID,
			sentDate: sentDate.Unix(),
		})

		m.parent.Opts.Log.Debugln(k.ID, "grouped per", subject, "at", sentDate)

		return nil
	})
	if err != nil {
		return nil, errors.New("Internal server error") // headerMetaScan logs the actual error
	}
	seqSet = nil // Hint for GC.

	for _, thread := range threads {
		sort.Slice(thread, func(i, j int) bool {
			return thread[i].sentDate < thread[j].sentDate
		})
	}
	sortedThreads := make([][]msg, 0, len(threads))
	for _, thread := range threads {
		sortedThreads = append(sortedThreads, thread)
	}
	threads = nil // Hint for GC.
	sort.Slice(sortedThreads, func(i, j int) bool {
		// Assertion: No empty threads (threads are only created by callback
		// above and have at least one message).
		return sortedThreads[i][0].sentDate < sortedThreads[j][0].sentDate
	})
	m.parent.Opts.Log.Debugln(len(sortedThreads), "threads", "msgCount:", msgCount)

	// We preallocate space for all Thread structures together
	// and then pick one at nodeOffset each set we need one.
	threadsTree := make([]sortthread.Thread, count)
	nodeOffset := 0
	result := make([]*sortthread.Thread, 0, len(threads))

	for _, thread := range sortedThreads {
		current := &threadsTree[nodeOffset]
		nodeOffset++
		result = append(result, current)
		// Assertion: No empty threads (threads are only created by callback
		// above and have at least one message).
		current.Id = thread[0].id
		for _, msg := range thread[1:] {
			next := &threadsTree[nodeOffset]
			nodeOffset++

			id := msg.id
			if !uid {
				var ok bool
				id, ok = m.handle.UidAsSeq(id)
				if !ok {
					continue // Wtf
				}
			}

			next.Id = id
			current.Children = []*sortthread.Thread{next}
			current = next
		}
	}

	return result, nil
}

func (m *Mailbox) headerMetaScan(tx *sql.Tx, seqSet *imap.SeqSet, callback func(k *msgKey) error) (int, error) {
	count := 0
	if tx == nil {
		var err error
		tx, err = m.parent.db.BeginLevel(sql.LevelReadCommitted, true)
		if err != nil {
			m.parent.logMboxErr(m, err, "headerMetaScan (tx start)", seqSet)
			return 0, err
		}
		defer tx.Rollback()
	}

outerLoop:
	for _, seq := range seqSet.Set {
		rows, err := tx.Stmt(m.parent.cachedHeaderUid).Query(m.id, seq.Start, seq.Stop)
		if err != nil {
			m.parent.logMboxErr(m, err, "headerMetaScan: cachedHeader", seqSet)
			return 0, err
		}

		for rows.Next() {
			var cachedHeaderBlob []byte
			key := msgKey{}
			if err := rows.Scan(&key.ID, &cachedHeaderBlob, &key.BodyLen, &key.ArrivalUnix); err != nil {
				m.parent.logMboxErr(m, err, "headerMetaScan: cachedHeader scan", seqSet)
				rows.Close()
				continue
			}
			if err := json.Unmarshal(cachedHeaderBlob, &key.CachedHeader); err != nil {
				m.parent.logMboxErr(m, err, "headerMetaScan: cachedHeader unmarshal", seqSet)
				rows.Close()
				continue
			}

			if err := callback(&key); err != nil {
				m.parent.logMboxErr(m, err, "headerMetaScan: callback error", seqSet)
				rows.Close()
				return 0, err
			}

			count++
			if count == 10000 {
				rows.Close()
				break outerLoop
			}
		}
		rows.Close()
	}

	return count, nil
}
