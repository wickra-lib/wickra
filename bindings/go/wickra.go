// Package wickra provides idiomatic Go bindings for the Wickra
// technical-analysis library over its C ABI hub.
//
// Each indicator is an opaque-handle type with a New<Indicator> constructor and
// Update/Batch/Reset/Close methods. Handles are freed by Close and, as a
// backstop, by a finalizer; call Close explicitly to release native memory
// promptly. The binding links against the prebuilt Wickra C ABI library, staged
// per platform under ./lib/<goos>_<goarch>/, with the C ABI header vendored
// under ./include. For distribution the libraries are committed alongside the
// source in the wickra-go module, so `go get` + `go build` works with no extra
// steps — see the package README.
package wickra

/*
#cgo CFLAGS: -I${SRCDIR}/include
#cgo linux,amd64 LDFLAGS: -L${SRCDIR}/lib/linux_amd64 -lwickra -Wl,-rpath,${SRCDIR}/lib/linux_amd64
#cgo linux,arm64 LDFLAGS: -L${SRCDIR}/lib/linux_arm64 -lwickra -Wl,-rpath,${SRCDIR}/lib/linux_arm64
#cgo darwin,amd64 LDFLAGS: -L${SRCDIR}/lib/darwin_amd64 -lwickra -Wl,-rpath,${SRCDIR}/lib/darwin_amd64
#cgo darwin,arm64 LDFLAGS: -L${SRCDIR}/lib/darwin_arm64 -lwickra -Wl,-rpath,${SRCDIR}/lib/darwin_arm64
#cgo windows,amd64 LDFLAGS: -L${SRCDIR}/lib/windows_amd64 -lwickra
#cgo windows,arm64 LDFLAGS: -L${SRCDIR}/lib/windows_arm64 -lwickra
#include "wickra.h"
*/
import "C"

import "errors"

// ErrInvalidParams is returned by a New<Indicator> constructor when the native
// constructor rejects the supplied parameters (for example a zero period).
var ErrInvalidParams = errors.New("wickra: invalid indicator parameters")

// ErrFeedClosed is returned by BinanceFeed.Next once the stream has been closed
// or has errored out and exhausted its reconnect attempts.
var ErrFeedClosed = errors.New("wickra: binance feed closed")
