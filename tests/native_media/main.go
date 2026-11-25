// Independent Pion controller for the Rust production WebRTC/H.264 path.
// No desktop pixels or OS input are accessed: the host example supplies testsrc2
// and writes validated control messages to a local file.
package main

import (
	"bufio"
	"bytes"
	"context"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha1"
	"encoding/base64"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/pion/interceptor"
	"github.com/pion/webrtc/v4"
	"github.com/pion/webrtc/v4/pkg/media/h264writer"
)

type options struct {
	host, relay, ffmpeg, output, mode string
	capture                           time.Duration
}
type caseResult struct {
	Mode                string                        `json:"mode"`
	Status              string                        `json:"status"`
	Error               string                        `json:"error,omitempty"`
	DurationMS          int64                         `json:"durationMs"`
	ReceivedRTP         uint64                        `json:"receivedRtpPackets"`
	RTPFrames           uint64                        `json:"rtpFrameMarkers"`
	DecodedFrames       int                           `json:"decodedFrames"`
	UniqueFrameHashes   int                           `json:"uniqueFrameHashes"`
	ControlEvents       int                           `json:"matchedControlEvents"`
	Capabilities        json.RawMessage               `json:"hostCapabilities,omitempty"`
	LocalCandidateType  string                        `json:"localCandidateType,omitempty"`
	RemoteCandidateType string                        `json:"remoteCandidateType,omitempty"`
	RelayProtocol       string                        `json:"relayProtocol,omitempty"`
	CandidatePairStats  *webrtc.ICECandidatePairStats `json:"selectedPairStats,omitempty"`
	Artifacts           string                        `json:"artifacts"`
}
type report struct {
	Timestamp   time.Time    `json:"timestamp"`
	Status      string       `json:"status"`
	Scope       string       `json:"scope"`
	PionVersion string       `json:"pionVersion"`
	Cases       []caseResult `json:"cases"`
}

var controlFrames = []string{
	`{"v":1,"type":"pointer","action":"move","x":0.25,"y":0.75,"buttons":0}`,
	`{"v":1,"type":"pointer","action":"down","x":0.25,"y":0.75,"buttons":1}`,
	`{"v":1,"type":"pointer","action":"up","x":0.25,"y":0.75,"buttons":0}`,
	`{"v":1,"type":"key","code":"Enter","down":true,"modifiers":0}`,
	`{"v":1,"type":"key","code":"Enter","down":false,"modifiers":0}`,
	`{"v":1,"type":"scroll","dx":0,"dy":120}`,
	`{"v":1,"type":"text","text":"Remote test తెలుగు ✓"}`,
	`{"v":1,"type":"release_all"}`,
}

func main() {
	var o options
	flag.StringVar(&o.host, "host", "../../agent-windows/target/debug/examples/synthetic_peer", "Rust synthetic peer executable")
	flag.StringVar(&o.relay, "relay", "../../artifacts/test-relay", "local test TURN fixture executable")
	flag.StringVar(&o.ffmpeg, "ffmpeg", "/usr/bin/ffmpeg", "ffmpeg for stream and decode")
	flag.StringVar(&o.output, "output", "../../evidence/media-native.json", "JSON evidence path")
	flag.StringVar(&o.mode, "mode", "all", "all, direct, udp or tcp")
	flag.DurationVar(&o.capture, "capture", 3*time.Second, "receive duration after connection (minimum 1s)")
	flag.Parse()
	if o.capture < time.Second || o.capture > 10*time.Second {
		fmt.Fprintln(os.Stderr, "capture must be 1s..10s")
		os.Exit(2)
	}
	modes := []string{"direct", "udp", "tcp"}
	if o.mode != "all" {
		if o.mode != "direct" && o.mode != "udp" && o.mode != "tcp" {
			fmt.Fprintln(os.Stderr, "unknown mode")
			os.Exit(2)
		}
		modes = []string{o.mode}
	}
	var err error
	for _, p := range []*string{&o.host, &o.relay, &o.ffmpeg, &o.output} {
		*p, err = filepath.Abs(*p)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(2)
		}
	}
	if err = os.MkdirAll(filepath.Dir(o.output), 0700); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	artifactRoot, err := os.MkdirTemp(filepath.Dir(o.output), "native-media-")
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	r := report{Timestamp: time.Now().UTC(), Status: "passed", PionVersion: "v4.2.26", Scope: "Synthetic source through Rust production H.264/WebRTC path to independent Pion receiver and ffmpeg decoder; no Windows capture/input, Android renderer, public Internet or TURN TLS validation", Cases: []caseResult{}}
	for _, mode := range modes {
		result := caseResult{Mode: mode, Status: "passed", Artifacts: filepath.Join(artifactRoot, mode)}
		_ = os.MkdirAll(result.Artifacts, 0700)
		start := time.Now()
		err = runCase(o, &result)
		result.DurationMS = time.Since(start).Milliseconds()
		if err != nil {
			result.Status = "failed"
			if strings.Contains(strings.ToLower(err.Error()), "operation not permitted") || strings.Contains(strings.ToLower(err.Error()), "permission denied") {
				result.Status = "blocked"
			}
			result.Error = err.Error()
			if result.Status == "failed" || r.Status == "passed" {
				r.Status = result.Status
			}
			fmt.Fprintf(os.Stderr, "%s: %s: %v\n", mode, result.Status, err)
		} else {
			fmt.Fprintf(os.Stderr, "%s: decoded %d frames (%d unique), control events %d, candidate=%s relayProtocol=%s\n", mode, result.DecodedFrames, result.UniqueFrameHashes, result.ControlEvents, result.LocalCandidateType, result.RelayProtocol)
		}
		r.Cases = append(r.Cases, result)
	}
	b, _ := json.MarshalIndent(r, "", "  ")
	b = append(b, '\n')
	if err = os.WriteFile(o.output, b, 0600); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	fmt.Println(string(b))
	if r.Status != "passed" {
		os.Exit(1)
	}
}
func waitFile(ctx context.Context, path string, done <-chan error) ([]byte, error) {
	tick := time.NewTicker(50 * time.Millisecond)
	defer tick.Stop()
	for {
		b, err := os.ReadFile(path)
		if err == nil && len(b) > 0 {
			return b, nil
		}
		if err != nil && !os.IsNotExist(err) {
			return nil, err
		}
		select {
		case <-ctx.Done():
			return nil, fmt.Errorf("waiting for %s: %w", filepath.Base(path), ctx.Err())
		case err := <-done:
			return nil, fmt.Errorf("peer exited before %s: %v", filepath.Base(path), err)
		case <-tick.C:
		}
	}
}
func stopProcess(cmd *exec.Cmd, done <-chan error) {
	if cmd == nil || cmd.Process == nil {
		return
	}
	_ = cmd.Process.Signal(os.Interrupt)
	select {
	case <-done:
		return
	case <-time.After(2 * time.Second):
		_ = cmd.Process.Kill()
		select {
		case <-done:
		case <-time.After(time.Second):
		}
	}
}

type relayURLs struct {
	UDP string `json:"udpURL"`
	TCP string `json:"tcpURL"`
}

func startRelay(ctx context.Context, o options, dir string) (relayURLs, string, func(), error) {
	key := make([]byte, 32)
	if _, err := rand.Read(key); err != nil {
		return relayURLs{}, "", nil, err
	}
	secret := base64.RawURLEncoding.EncodeToString(key)
	log, err := os.Create(filepath.Join(dir, "relay.log"))
	if err != nil {
		return relayURLs{}, "", nil, err
	}
	cmd := exec.CommandContext(ctx, o.relay)
	cmd.Env = append(os.Environ(), "OPENREMOTE_TEST_TURN_SECRET="+secret)
	cmd.Stderr = log
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		log.Close()
		return relayURLs{}, "", nil, err
	}
	if err = cmd.Start(); err != nil {
		log.Close()
		return relayURLs{}, "", nil, err
	}
	done := make(chan error, 1)
	go func() { done <- cmd.Wait() }()
	cleanup := func() { stopProcess(cmd, done); log.Close() }
	result := make(chan struct {
		urls relayURLs
		err  error
	}, 1)
	go func() {
		var urls relayURLs
		err := json.NewDecoder(stdout).Decode(&urls)
		result <- struct {
			urls relayURLs
			err  error
		}{urls, err}
	}()
	select {
	case r := <-result:
		if r.err != nil {
			cleanup()
			b, _ := os.ReadFile(filepath.Join(dir, "relay.log"))
			return relayURLs{}, "", nil, fmt.Errorf("relay fixture: %w; %s", r.err, b)
		}
		return r.urls, secret, cleanup, nil
	case <-ctx.Done():
		cleanup()
		return relayURLs{}, "", nil, ctx.Err()
	}
}
func runCase(o options, result *caseResult) error {
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Second)
	defer cancel()
	config := webrtc.Configuration{ICETransportPolicy: webrtc.ICETransportPolicyAll}
	if result.Mode != "direct" {
		urls, secret, cleanup, err := startRelay(ctx, o, result.Artifacts)
		if err != nil {
			return err
		}
		defer cleanup()
		url := urls.UDP
		if result.Mode == "tcp" {
			url = urls.TCP
		}
		username := fmt.Sprintf("%d:native-test", time.Now().Add(5*time.Minute).Unix())
		mac := hmac.New(sha1.New, []byte(secret))
		_, _ = mac.Write([]byte(username))
		config.ICEServers = []webrtc.ICEServer{{URLs: []string{url}, Username: username, Credential: base64.StdEncoding.EncodeToString(mac.Sum(nil))}}
		config.ICETransportPolicy = webrtc.ICETransportPolicyRelay
	}
	media := &webrtc.MediaEngine{}
	if err := media.RegisterCodec(webrtc.RTPCodecParameters{RTPCodecCapability: webrtc.RTPCodecCapability{MimeType: webrtc.MimeTypeH264, ClockRate: 90000, SDPFmtpLine: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f", RTCPFeedback: []webrtc.RTCPFeedback{{Type: "nack"}, {Type: "nack", Parameter: "pli"}}}, PayloadType: 102}, webrtc.RTPCodecTypeVideo); err != nil {
		return err
	}
	interceptors := &interceptor.Registry{}
	if err := webrtc.RegisterDefaultInterceptors(media, interceptors); err != nil {
		return err
	}
	settings := webrtc.SettingEngine{}
	settings.SetIncludeLoopbackCandidate(true)
	settings.SetNetworkTypes([]webrtc.NetworkType{webrtc.NetworkTypeUDP4})
	api := webrtc.NewAPI(webrtc.WithMediaEngine(media), webrtc.WithInterceptorRegistry(interceptors), webrtc.WithSettingEngine(settings))
	pc, err := api.NewPeerConnection(config)
	if err != nil {
		return err
	}
	defer pc.Close()
	connected := make(chan struct{})
	var connectOnce sync.Once
	pc.OnConnectionStateChange(func(s webrtc.PeerConnectionState) {
		if s == webrtc.PeerConnectionStateConnected {
			connectOnce.Do(func() { close(connected) })
		}
	})
	if _, err = pc.AddTransceiverFromKind(webrtc.RTPCodecTypeVideo, webrtc.RTPTransceiverInit{Direction: webrtc.RTPTransceiverDirectionRecvonly}); err != nil {
		return err
	}
	dc, err := pc.CreateDataChannel("control", nil)
	if err != nil {
		return err
	}
	channelReady := make(chan struct{})
	dc.OnOpen(func() { close(channelReady) })
	capabilities := make(chan []byte, 1)
	dc.OnMessage(func(m webrtc.DataChannelMessage) {
		if m.IsString {
			select {
			case capabilities <- append([]byte(nil), m.Data...):
			default:
			}
		}
	})
	capturePath := filepath.Join(result.Artifacts, "received.h264")
	writer, err := h264writer.New(capturePath)
	if err != nil {
		return err
	}
	var packets, markers atomic.Uint64
	var captureStop atomic.Bool
	trackReady := make(chan struct{})
	trackDone := make(chan struct{})
	readErr := make(chan error, 1)
	var trackOnce sync.Once
	pc.OnTrack(func(track *webrtc.TrackRemote, _ *webrtc.RTPReceiver) {
		accepted := false
		trackOnce.Do(func() { accepted = true; close(trackReady) })
		if !accepted {
			return
		}
		defer close(trackDone)
		if !strings.EqualFold(track.Codec().MimeType, webrtc.MimeTypeH264) {
			readErr <- fmt.Errorf("unexpected codec %s", track.Codec().MimeType)
			return
		}
		for {
			p, _, err := track.ReadRTP()
			if err != nil {
				return
			}
			if err = writer.WriteRTP(p); err != nil {
				readErr <- err
				return
			}
			packets.Add(1)
			if p.Marker {
				markers.Add(1)
				if captureStop.Load() {
					return
				}
			}
		}
	})
	defer func() {
		_ = pc.Close()
		select {
		case <-trackReady:
			select {
			case <-trackDone:
			case <-time.After(time.Second):
			}
		default:
		}
		_ = writer.Close()
	}()
	gathering := webrtc.GatheringCompletePromise(pc)
	offer, err := pc.CreateOffer(nil)
	if err != nil {
		return err
	}
	if err = pc.SetLocalDescription(offer); err != nil {
		return err
	}
	select {
	case <-gathering:
	case <-ctx.Done():
		return fmt.Errorf("ICE gathering: %w", ctx.Err())
	}
	local := pc.LocalDescription()
	if local == nil || !strings.Contains(local.SDP, "a=candidate:") {
		return errors.New("controller gathered no ICE candidates")
	}
	if result.Mode != "direct" {
		for _, line := range strings.Split(local.SDP, "\n") {
			if strings.HasPrefix(line, "a=candidate:") && !strings.Contains(line, " typ relay") {
				return fmt.Errorf("relay-only offer contained nonrelay candidate: %s", line)
			}
		}
	}
	offerPath := filepath.Join(result.Artifacts, "offer.json")
	answerPath := filepath.Join(result.Artifacts, "answer.json")
	eventsPath := filepath.Join(result.Artifacts, "events.jsonl")
	b, _ := json.Marshal(local)
	if err = os.WriteFile(offerPath, b, 0600); err != nil {
		return err
	}
	hostLog, err := os.Create(filepath.Join(result.Artifacts, "host.log"))
	if err != nil {
		return err
	}
	defer hostLog.Close()
	host := exec.CommandContext(ctx, o.host, "--offer-file", offerPath, "--answer-file", answerPath, "--events-file", eventsPath, "--ffmpeg", o.ffmpeg, "--seconds", "45")
	host.Stdout = hostLog
	host.Stderr = hostLog
	if err = host.Start(); err != nil {
		return err
	}
	hostDone := make(chan error, 1)
	go func() { hostDone <- host.Wait() }()
	defer stopProcess(host, hostDone)
	answerBytes, err := waitFile(ctx, answerPath, hostDone)
	if err != nil {
		b, _ := os.ReadFile(filepath.Join(result.Artifacts, "host.log"))
		return fmt.Errorf("%w; host log: %s", err, b)
	}
	var answer webrtc.SessionDescription
	if err = json.Unmarshal(answerBytes, &answer); err != nil {
		return err
	}
	if err = pc.SetRemoteDescription(answer); err != nil {
		return err
	}
	for _, ready := range []<-chan struct{}{connected, channelReady, trackReady} {
		select {
		case <-ready:
		case err := <-hostDone:
			b, _ := os.ReadFile(filepath.Join(result.Artifacts, "host.log"))
			return fmt.Errorf("host exited before media: %v; %s", err, b)
		case <-ctx.Done():
			return fmt.Errorf("peer/channel/track connection: %w", ctx.Err())
		}
	}
	select {
	case caps := <-capabilities:
		var data map[string]any
		if json.Unmarshal(caps, &data) != nil || data["type"] != "capabilities" || data["fileTransfer"] != false || data["audio"] != false {
			return fmt.Errorf("unexpected capabilities %s", caps)
		}
		result.Capabilities = append([]byte(nil), caps...)
	case <-ctx.Done():
		return errors.New("missing capabilities")
	}
	for _, frame := range controlFrames {
		if err = dc.SendText(frame); err != nil {
			return err
		}
	}
	timer := time.NewTimer(o.capture)
	defer timer.Stop()
	select {
	case <-timer.C:
	case err := <-readErr:
		return fmt.Errorf("H264 depacketize: %w", err)
	case err := <-hostDone:
		return fmt.Errorf("host prematurely exited: %v", err)
	case <-ctx.Done():
		return ctx.Err()
	}
	captureStop.Store(true)
	select {
	case <-trackDone:
	case <-ctx.Done():
		return errors.New("recording did not finish an RTP frame")
	}
	result.ReceivedRTP = packets.Load()
	result.RTPFrames = markers.Load()
	transport := pc.SCTP().Transport().ICETransport()
	pair, err := transport.GetSelectedCandidatePair()
	if err != nil || pair == nil {
		return fmt.Errorf("missing selected candidate pair: %v", err)
	}
	result.LocalCandidateType = pair.Local.Typ.String()
	result.RemoteCandidateType = pair.Remote.Typ.String()
	pairStats, ok := transport.GetSelectedCandidatePairStats()
	if !ok {
		return errors.New("missing selected-pair statistics")
	}
	result.CandidatePairStats = &pairStats
	for _, stat := range pc.GetStats() {
		if candidate, ok := stat.(webrtc.ICECandidateStats); ok && candidate.ID == pairStats.LocalCandidateID {
			result.RelayProtocol = candidate.RelayProtocol
		}
	}
	if result.Mode == "direct" {
		if result.LocalCandidateType == "relay" {
			return errors.New("direct case unexpectedly relayed")
		}
	} else if result.LocalCandidateType != "relay" || result.RelayProtocol != result.Mode {
		return fmt.Errorf("requested %s relay but observed candidate=%s relayProtocol=%s", result.Mode, result.LocalCandidateType, result.RelayProtocol)
	}
	events, err := os.ReadFile(eventsPath)
	if err != nil {
		return err
	}
	lines := strings.Split(strings.TrimSpace(string(events)), "\n")
	if len(lines) != len(controlFrames) {
		return fmt.Errorf("got %d input events, expected %d", len(lines), len(controlFrames))
	}
	for i, frame := range controlFrames {
		if lines[i] != frame {
			return fmt.Errorf("control event %d mismatched: %s", i, lines[i])
		}
	}
	result.ControlEvents = len(lines)
	_ = pc.Close()
	select {
	case <-trackDone:
	case <-ctx.Done():
		return errors.New("track reader did not stop")
	}
	if err = writer.Close(); err != nil {
		return err
	}
	decodeCtx, stopDecode := context.WithTimeout(ctx, 10*time.Second)
	defer stopDecode()
	decode := exec.CommandContext(decodeCtx, o.ffmpeg, "-hide_banner", "-v", "error", "-f", "h264", "-i", capturePath, "-an", "-f", "framemd5", "-")
	var decodeErrors bytes.Buffer
	decode.Stderr = &decodeErrors
	hashes, err := decode.Output()
	_ = os.WriteFile(filepath.Join(result.Artifacts, "decode.log"), decodeErrors.Bytes(), 0600)
	if err != nil {
		return fmt.Errorf("ffmpeg decode: %w; %s", err, decodeErrors.String())
	}
	if decodeErrors.Len() != 0 {
		return fmt.Errorf("ffmpeg reported corrupt media: %s", decodeErrors.String())
	}
	_ = os.WriteFile(filepath.Join(result.Artifacts, "frames.md5"), hashes, 0600)
	unique := map[string]bool{}
	scanner := bufio.NewScanner(bytes.NewReader(hashes))
	for scanner.Scan() {
		line := scanner.Text()
		if strings.HasPrefix(line, "#") || strings.TrimSpace(line) == "" {
			continue
		}
		parts := strings.Split(line, ",")
		if len(parts) != 6 {
			return fmt.Errorf("invalid framemd5 row: %s", line)
		}
		result.DecodedFrames++
		unique[strings.TrimSpace(parts[5])] = true
	}
	if err = scanner.Err(); err != nil {
		return err
	}
	result.UniqueFrameHashes = len(unique)
	if result.DecodedFrames < 10 || result.UniqueFrameHashes < 10 {
		return fmt.Errorf("media not moving: decoded=%d distinct=%d", result.DecodedFrames, result.UniqueFrameHashes)
	}
	return nil
}
