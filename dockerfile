####################################################################################################
## Builder
####################################################################################################
FROM rust:latest AS builder

# RUN rustup target add x86_64-unknown-linux-musl
# RUN apt update && apt install -y libssl-dev musl-tools musl-dev
RUN update-ca-certificates

# Install sqlx-cli
# RUN cargo install sqlx-cli --no-default-features --features native-tls,postgres

WORKDIR /painee

COPY ./Cargo.toml ./Cargo.lock ./

RUN mkdir src
RUN echo "fn main() {}" > ./src/main.rs

RUN cargo build --release

COPY . .

RUN mkdir res
RUN cp -r ./pages ./styles ./assets ./migrations ./res
RUN touch ./src/main.rs && cargo build --release

####################################################################################################
## Final image
####################################################################################################
FROM gcr.io/distroless/cc

EXPOSE 8080

WORKDIR /painee

# Copy our build
COPY --from=builder /painee/res /painee/target/release/painee ./

CMD ["/painee/painee"]