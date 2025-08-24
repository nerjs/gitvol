FROM rust:alpine AS builder
RUN apk add --no-cache musl-dev
    
WORKDIR /gitvol
COPY . .
RUN cargo build --release

FROM alpine:latest
WORKDIR /gitvol
COPY --from=builder /gitvol/target/release/gitvol /gitvol/gitvol

RUN apk add --no-cache git

RUN ["/gitvol/gitvol", "--version"]

CMD [ "-s", "/run/docker/plugins/gitvol.sock", "-m", "/gitvol/volumes" ]
ENTRYPOINT [ "/gitvol/gitvol" ]