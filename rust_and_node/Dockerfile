FROM rust

RUN rustup component add rustfmt

RUN curl -fsSL https://deb.nodesource.com/setup_16.x | bash - && \
    apt install -y nodejs && \
    node -v