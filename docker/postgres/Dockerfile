FROM apache/age:release_PG17_1.6.0

# Install pgvector from PG17 apt packages
RUN apt-get update && \
    apt-get install -y postgresql-17-pgvector && \
    rm -rf /var/lib/apt/lists/*
