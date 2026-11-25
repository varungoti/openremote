// Local integration fixture only; never deploy as the product TURN server.
package main

import (
	"context"
	"crypto/hmac"
	"crypto/sha1"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"net"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/pion/turn/v4"
)

func main() {
	// A short lifetime and loopback bind prevent this fixture becoming a public relay.
	secret := os.Getenv("OPENREMOTE_TEST_TURN_SECRET")
	if len(secret) < 32 {
		panic("set a random OPENREMOTE_TEST_TURN_SECRET of at least 32 characters")
	}
	udp, err := net.ListenPacket("udp4", "127.0.0.1:0")
	if err != nil {
		panic(err)
	}
	defer udp.Close()
	tcp, err := net.Listen("tcp4", "127.0.0.1:0")
	if err != nil {
		panic(err)
	}
	defer tcp.Close()
	realm := "openremote-integration"
	generator := &turn.RelayAddressGeneratorStatic{RelayAddress: net.ParseIP("127.0.0.1"), Address: "127.0.0.1"}
	srv, err := turn.NewServer(turn.ServerConfig{
		Realm: realm,
		AuthHandler: func(username, gotRealm string, src net.Addr) ([]byte, bool) {
			if gotRealm != realm {
				return nil, false
			}
			raw, _, ok := strings.Cut(username, ":")
			if !ok {
				return nil, false
			}
			expiry, err := strconv.ParseInt(raw, 10, 64)
			if err != nil || expiry < time.Now().Unix() || expiry > time.Now().Add(time.Hour).Unix() {
				return nil, false
			}
			mac := hmac.New(sha1.New, []byte(secret))
			mac.Write([]byte(username))
			return turn.GenerateAuthKey(username, realm, base64.StdEncoding.EncodeToString(mac.Sum(nil))), true
		},
		PacketConnConfigs: []turn.PacketConnConfig{{PacketConn: udp, RelayAddressGenerator: generator}},
		ListenerConfigs:   []turn.ListenerConfig{{Listener: tcp, RelayAddressGenerator: generator}},
	})
	if err != nil {
		panic(err)
	}
	defer srv.Close()
	fmt.Fprintln(os.Stderr, "OpenRemote LOCAL TEST relay; automatic shutdown after 60 seconds")
	json.NewEncoder(os.Stdout).Encode(map[string]any{"udpURL": "turn:" + udp.LocalAddr().String() + "?transport=udp", "tcpURL": "turn:" + tcp.Addr().String() + "?transport=tcp"})
	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer cancel()
	select {
	case <-ctx.Done():
	case <-time.After(60 * time.Second):
	}
}
