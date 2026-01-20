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

package clitools

import (
	"bufio"
	"crypto/ed25519"
	"errors"
	"fmt"
	"os"
)

var stdinScanner = bufio.NewScanner(os.Stdin)

func Confirmation(prompt string, def bool) bool {
	selection := "y/N"
	if def {
		selection = "Y/n"
	}

	fmt.Fprintf(os.Stderr, "%s [%s]: ", prompt, selection)
	if !stdinScanner.Scan() {
		fmt.Fprintln(os.Stderr, stdinScanner.Err())
		return false
	}

	switch stdinScanner.Text() {
	case "Y", "y":
		return true
	case "N", "n":
		return false
	default:
		return def
	}
}

func readPass(tty *os.File, output []byte) ([]byte, error) {
	cursor := output[0:1]
	readen := 0
	for {
		n, err := tty.Read(cursor)
		if n != 1 {
			return nil, errors.New("ReadPassword: invalid read size when not in canonical mode")
		}
		if err != nil {
			return nil, errors.New("ReadPassword: " + err.Error())
		}
		if cursor[0] == '\n' {
			break
		}
		// Esc or Ctrl+D or Ctrl+C.
		if cursor[0] == '\x1b' || cursor[0] == '\x04' || cursor[0] == '\x03' {
			return nil, errors.New("ReadPassword: prompt rejected")
		}
		if cursor[0] == '\x7F' /* DEL */ {
			if readen != 0 {
				readen--
				cursor = output[readen : readen+1]
			}
			continue
		}

		if readen == cap(output) {
			return nil, errors.New("ReadPassword: too long password")
		}

		readen++
		cursor = output[readen : readen+1]
	}

	return output[0:readen], nil
}

func ReadPassword(prompt string) (string, error) {
	termios, err := TurnOnRawIO(os.Stdin)
	hiddenPass := true
	if err != nil {
		hiddenPass = false
		fmt.Fprintln(os.Stderr, "Failed to disable terminal output:", err)
	}

	// There is no meaningful way to handle error here.
	//nolint:errcheck
	defer TcSetAttr(os.Stdin.Fd(), &termios)

	fmt.Fprintf(os.Stderr, "%s: ", prompt)

	if hiddenPass {
		buf := make([]byte, 512)
		buf, err = readPass(os.Stdin, buf)
		if err != nil {
			return "", err
		}
		fmt.Println()

		return string(buf), nil
	}
	if !stdinScanner.Scan() {
		return "", stdinScanner.Err()
	}

	return stdinScanner.Text(), nil
}

// VerifySignature checks if the file at the given path has a valid Ed25519 signature
// appended to it. The signature must be the last 64 bytes of the file.
func VerifySignature(path string, publicKey []byte) (bool, error) {
	f, err := os.Open(path)
	if err != nil {
		return false, err
	}
	defer f.Close()

	info, err := f.Stat()
	if err != nil {
		return false, err
	}

	size := info.Size()
	if size < ed25519.SignatureSize {
		return false, fmt.Errorf("file too small to contain a signature")
	}

	// Read the signature (last 64 bytes)
	sig := make([]byte, ed25519.SignatureSize)
	if _, err := f.ReadAt(sig, size-int64(ed25519.SignatureSize)); err != nil {
		return false, err
	}

	// Read the content (all bytes except the last 64)
	contentSize := size - int64(ed25519.SignatureSize)
	content := make([]byte, contentSize)
	if _, err := f.ReadAt(content, 0); err != nil {
		return false, err
	}

	// Verify the signature
	return ed25519.Verify(publicKey, content, sig), nil
}
