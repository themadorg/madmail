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

package updatepipe

import (
	"bufio"
	"fmt"
	"io"
	"net"
	"os"
	"sync"

	mess "github.com/foxcpp/go-imap-mess"
	"github.com/themadorg/madmail/framework/log"
)

// UnixSockPipe implements the UpdatePipe interface by serializating updates
// to/from a Unix domain socket. Due to the way Unix sockets work, only one
// Listen goroutine can be running.
//
// The socket is stream-oriented and consists of the following messages:
//
//	SENDER_ID;JSON_SERIALIZED_INTERNAL_OBJECT\n
//
// And SENDER_ID is Process ID and UnixSockPipe address concated as a string.
// It is used to deduplicate updates sent to Push and recevied via Listen.
//
// The SockPath field specifies the socket path to use. The actual socket
// is initialized on the first call to Listen or (Init)Push.
type UnixSockPipe struct {
	SockPath string
	Log      log.Logger

	mu       sync.Mutex
	listener net.Listener
	sender   net.Conn
	closed   bool
}

var _ P = &UnixSockPipe{}

func (usp *UnixSockPipe) myID() string {
	return fmt.Sprintf("%d-%p", os.Getpid(), usp)
}

func (usp *UnixSockPipe) readUpdates(conn net.Conn, updCh chan<- mess.Update) {
	defer conn.Close()
	scnr := bufio.NewScanner(conn)
	for scnr.Scan() {
		id, upd, err := parseUpdate(scnr.Text())
		if err != nil {
			usp.Log.Error("malformed update received", err, "str", scnr.Text())
			continue
		}

		// It is our own update, skip.
		if id == usp.myID() {
			continue
		}

		updCh <- *upd
	}
}

func (usp *UnixSockPipe) Listen(upd chan<- mess.Update) error {
	usp.mu.Lock()
	defer usp.mu.Unlock()

	if usp.listener != nil {
		return fmt.Errorf("Listen already called")
	}

	l, err := net.Listen("unix", usp.SockPath)
	if err != nil {
		return err
	}
	usp.listener = l
	go func() {
		for {
			conn, err := l.Accept()
			if err != nil {
				usp.mu.Lock()
				closed := usp.closed
				usp.mu.Unlock()
				if closed {
					return
				}
				usp.Log.Error("accept failed", err)
				return
			}
			go usp.readUpdates(conn, upd)
		}
	}()
	return nil
}

func (usp *UnixSockPipe) InitPush() error {
	usp.mu.Lock()
	defer usp.mu.Unlock()

	if usp.sender != nil {
		return nil
	}

	sock, err := net.Dial("unix", usp.SockPath)
	if err != nil {
		return err
	}

	usp.sender = sock
	return nil
}

func (usp *UnixSockPipe) Push(upd mess.Update) error {
	usp.mu.Lock()
	if usp.sender == nil {
		usp.mu.Unlock()
		if err := usp.InitPush(); err != nil {
			return err
		}
		usp.mu.Lock()
	}
	sender := usp.sender
	usp.mu.Unlock()

	updStr, err := formatUpdate(usp.myID(), upd)
	if err != nil {
		return err
	}

	_, err = io.WriteString(sender, updStr)
	return err
}

func (usp *UnixSockPipe) Close() error {
	usp.mu.Lock()
	defer usp.mu.Unlock()

	usp.closed = true
	if usp.sender != nil {
		usp.sender.Close()
	}
	if usp.listener != nil {
		usp.listener.Close()
		os.Remove(usp.SockPath)
	}
	return nil
}
