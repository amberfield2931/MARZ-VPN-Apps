/* SPDX-License-Identifier: MIT
 *
 * Copyright (C) 2017-2021 WireGuard LLC. All Rights Reserved.
 *
 * Modified by the MARZ VPN project, 2026-11-03: loads the unmodified wintun.dll installed side by side with this
 * executable through the Windows loader, instead of an embedded copy through memmod. No other change.
 */

package wintun

import (
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
	"unsafe"

	"golang.org/x/sys/windows"
)

func (d *lazyDLL) NewProc(name string) *lazyProc {
	return &lazyProc{dll: d, Name: name}
}

type lazyProc struct {
	Name string
	mu   sync.Mutex
	dll  *lazyDLL
	addr uintptr
}

func (p *lazyProc) Find() error {
	if atomic.LoadPointer((*unsafe.Pointer)(unsafe.Pointer(&p.addr))) != nil {
		return nil
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.addr != 0 {
		return nil
	}

	err := p.dll.Load()
	if err != nil {
		return fmt.Errorf("error loading DLL: %s, MODULE: %s, error: %w", p.dll.Name, p.Name, err)
	}
	addr, err := p.nameToAddr()
	if err != nil {
		return fmt.Errorf("error getting %s address: %w", p.Name, err)
	}

	atomic.StorePointer((*unsafe.Pointer)(unsafe.Pointer(&p.addr)), unsafe.Pointer(addr))
	return nil
}

func (p *lazyProc) Addr() uintptr {
	err := p.Find()
	if err != nil {
		panic(err)
	}
	return p.addr
}

func (p *lazyProc) Load() error {
	return p.dll.Load()
}

type lazyDLL struct {
	Name   string
	Base   windows.Handle
	mu     sync.Mutex
	loaded uint32
}

func newLazyDLL(name string) *lazyDLL {
	return &lazyDLL{Name: name}
}

func (d *lazyDLL) Load() error {
	if atomic.LoadUint32(&d.loaded) != 0 {
		return nil
	}
	d.mu.Lock()
	defer d.mu.Unlock()
	if d.loaded != 0 {
		return nil
	}
	exe, err := os.Executable()
	if err != nil {
		return fmt.Errorf("unable to find the program folder: %w", err)
	}
	// An absolute path in the program's own folder, never the current directory or PATH.
	base, err := windows.LoadLibraryEx(filepath.Join(filepath.Dir(exe), "wintun.dll"), 0, windows.LOAD_WITH_ALTERED_SEARCH_PATH)
	if err != nil {
		return fmt.Errorf("unable to load library: %w", err)
	}
	d.Base = base
	atomic.StoreUint32(&d.loaded, 1)
	return nil
}

func (p *lazyProc) nameToAddr() (uintptr, error) {
	return windows.GetProcAddress(p.dll.Base, p.Name)
}
