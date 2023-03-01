FROM ubuntu:22.04
 
RUN apt-get update && apt-get install -y curl
RUN apt-get install build-essential git libssl-dev -y
 
# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install Go
RUN apt-get install golang-go -y
ENV PATH="/root/go/bin:${PATH}"

# Install Litestream
RUN git clone https://github.com/fmeringdal/litestream.git /home/ubuntu/litestream

WORKDIR /home/ubuntu/litestream
RUN go install -tags sqlcipher ./cmd/litestream
RUN litestream version

# Install Covert
WORKDIR /home/ubuntu/covert
COPY . .
RUN mkdir /root/covert
RUN cp config.example.toml /root/covert/config.toml
RUN cargo install --path covert-cli

CMD ["covert", "server", "--config", "/root/covert/config.toml"]
