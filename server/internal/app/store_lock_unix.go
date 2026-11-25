//go:build linux || darwin || freebsd || openbsd || netbsd || dragonfly

package app

import (
	"fmt"
	"os"
	"syscall"
)

func acquireStateLock(path string) (*os.File, error) {
	// O_NOFOLLOW prevents redirecting the lock to another file through a symlink.
	fd, err := syscall.Open(path, syscall.O_RDWR|syscall.O_CREAT|syscall.O_CLOEXEC|syscall.O_NOFOLLOW, 0600)
	if err != nil {
		return nil, fmt.Errorf("open state lock: %w", err)
	}
	f := os.NewFile(uintptr(fd), path)
	if err = syscall.Flock(fd, syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		_ = f.Close()
		return nil, fmt.Errorf("state already in use by another process: %w", err)
	}
	return f, nil
}
