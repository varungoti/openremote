package app

import (
	"errors"
	"os"
)

func acquireStateLock(path string) (*os.File, error) {
	return nil, errors.New("the durable coordinator is supported on Linux; run its container or Linux binary")
}
