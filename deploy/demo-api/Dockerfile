# Builds the demo API as one static binary and runs it on an empty image,
# as nobody. Build from the repository root (compose.yml does).
FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p crane-demo-api

FROM scratch
COPY --from=build /src/target/release/crane-demo-api /crane-demo-api
USER 65534:65534
ENV BIND=0.0.0.0:8080 TRUST_PROXY=1
EXPOSE 8080
ENTRYPOINT ["/crane-demo-api"]
