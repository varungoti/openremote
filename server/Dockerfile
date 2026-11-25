FROM golang:1.27.4-alpine AS build
WORKDIR /src
COPY go.mod ./
COPY . .
RUN CGO_ENABLED=0 go test ./... && \
    CGO_ENABLED=0 go build -trimpath -ldflags='-s -w' -o /out/controlplane ./cmd/controlplane && \
    mkdir -p /out/data && chmod 700 /out/data
FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /out/controlplane /controlplane
COPY --chown=65532:65532 --from=build /out/data /data
ENV OPENREMOTE_LISTEN=0.0.0.0:8080 OPENREMOTE_STATE_FILE=/data/controlplane.json
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/controlplane"]
