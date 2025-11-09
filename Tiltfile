allow_k8s_contexts('admin@talos')

# Read BSR token from environment (managed by mise)
bsr_token = os.getenv('BSR_TOKEN', '')

# Build the Docker image with BSR authentication
# Pass the token directly as an environment variable to avoid file management issues
docker_build(
    'ponix-all-in-one:latest',
    context='.',
    dockerfile='./crates/ponix-all-in-one/Dockerfile',
    secret=['id=bsr_token,env=BSR_TOKEN'],
    only=[
        './.cargo',
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
