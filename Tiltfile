# Build the Docker image
docker_build(
    'ponix-all-in-one:latest',
    context='.',
    dockerfile='./crates/ponix-all-in-one/Dockerfile',
    only=[
        './Cargo.toml',
        './Cargo.lock',
        './crates',
    ],
    ignore=[
        '**/target',
        '**/*.md',
        '**/tests',
    ]
)

# Run via Docker Compose
docker_compose('./docker/docker-compose.service.yaml')
