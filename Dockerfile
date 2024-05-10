FROM rust:latest AS build
ADD . /build
RUN apt install libssl-dev
RUN cd /build && cargo build --release --bin soulfire

FROM debian:12-slim
RUN apt update && apt install -y ca-certificates
RUN mkdir -p /usr/local/ssl && ln -s /etc/ssl/certs/ /usr/local/ssl/certs
RUN mkdir /app
COPY --from=build /build/target/release/soulfire /app/
ADD templates /app/templates
ADD games /app/games

EXPOSE 8000/tcp
WORKDIR /app
CMD ["./soulfire"]
