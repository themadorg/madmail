package clitools

import (
	"compress/bzip2"
	"encoding/binary"
	"fmt"
	"io"
	"os"
)

// ApplyDeltaPatch applies a bsdiff40 patch file to oldBinPath, writing the
// result to newBinPath. The caller is responsible for verifying the
// signature of the resulting binary.
func ApplyDeltaPatch(oldBinPath, patchPath, newBinPath string) error {
	oldData, err := os.ReadFile(oldBinPath)
	if err != nil {
		return fmt.Errorf("read old binary: %w", err)
	}

	patchData, err := os.ReadFile(patchPath)
	if err != nil {
		return fmt.Errorf("read patch: %w", err)
	}

	newData, err := bspatch(oldData, patchData)
	if err != nil {
		return fmt.Errorf("apply patch: %w", err)
	}

	if err := os.WriteFile(newBinPath, newData, 0755); err != nil {
		return fmt.Errorf("write patched binary: %w", err)
	}

	return nil
}

// bspatch applies a BSDIFF40 patch to old and returns the new data.
// See https://www.daemonology.net/bsdiff/ for the format description.
func bspatch(old, patch []byte) ([]byte, error) {
	if len(patch) < 32 {
		return nil, fmt.Errorf("patch too small")
	}
	if string(patch[:8]) != "BSDIFF40" {
		return nil, fmt.Errorf("invalid magic: %q", string(patch[:8]))
	}

	ctrlLen := int64(binary.LittleEndian.Uint64(patch[8:16]))
	diffLen := int64(binary.LittleEndian.Uint64(patch[16:24]))
	newLen := int64(binary.LittleEndian.Uint64(patch[24:32]))

	if ctrlLen < 0 || diffLen < 0 || newLen < 0 {
		return nil, fmt.Errorf("invalid patch header values")
	}

	const headerSize = 32
	if int64(len(patch)) < headerSize+ctrlLen+diffLen {
		return nil, fmt.Errorf("patch data too short")
	}

	ctrlBlock := patch[headerSize : headerSize+ctrlLen]
	diffBlock := patch[headerSize+ctrlLen : headerSize+ctrlLen+diffLen]
	extraBlock := patch[headerSize+ctrlLen+diffLen:]

	ctrlSlice := bytesReader(ctrlBlock)
	diffSlice := bytesReader(diffBlock)
	extraSlice := bytesReader(extraBlock)
	ctrlReader := bzip2.NewReader(&ctrlSlice)
	diffReader := bzip2.NewReader(&diffSlice)
	extraReader := bzip2.NewReader(&extraSlice)

	newData := make([]byte, newLen)
	oldLen := int64(len(old))
	var oldPos, newPos int64

	for newPos < newLen {
		// Read control triple: (x, y, z)
		x, err := readOffT(ctrlReader)
		if err != nil {
			return nil, fmt.Errorf("read ctrl x: %w", err)
		}
		y, err := readOffT(ctrlReader)
		if err != nil {
			return nil, fmt.Errorf("read ctrl y: %w", err)
		}
		z, err := readOffT(ctrlReader)
		if err != nil {
			return nil, fmt.Errorf("read ctrl z: %w", err)
		}

		// Add x bytes from diff to old.
		if newPos+x > newLen {
			return nil, fmt.Errorf("ctrl x overflows new file")
		}
		diffBuf := make([]byte, x)
		if _, err := io.ReadFull(diffReader, diffBuf); err != nil {
			return nil, fmt.Errorf("read diff data: %w", err)
		}
		for i := int64(0); i < x; i++ {
			np := newPos + i
			op := oldPos + i
			b := diffBuf[i]
			if op >= 0 && op < oldLen {
				b += old[op]
			}
			newData[np] = b
		}
		newPos += x
		oldPos += x

		// Copy y bytes from extra.
		if newPos+y > newLen {
			return nil, fmt.Errorf("ctrl y overflows new file")
		}
		if _, err := io.ReadFull(extraReader, newData[newPos:newPos+y]); err != nil {
			return nil, fmt.Errorf("read extra data: %w", err)
		}
		newPos += y
		oldPos += z
	}

	return newData, nil
}

// readOffT reads a signed 64-bit offset from the bsdiff stream.
// bsdiff encodes integers with the sign bit in bit 63 of the last byte,
// independent of two's complement (i.e., sign-magnitude encoding).
func readOffT(r io.Reader) (int64, error) {
	buf := make([]byte, 8)
	if _, err := io.ReadFull(r, buf); err != nil {
		return 0, err
	}
	// Read magnitude from low 63 bits.
	y := int64(binary.LittleEndian.Uint64(buf) & 0x7fffffffffffffff)
	if buf[7]&0x80 != 0 {
		y = -y
	}
	return y, nil
}

// bytesReader wraps a byte slice in an io.Reader (avoids bytes import).
type bytesReader []byte

func (b *bytesReader) Read(p []byte) (int, error) {
	if len(*b) == 0 {
		return 0, io.EOF
	}
	n := copy(p, *b)
	*b = (*b)[n:]
	return n, nil
}
