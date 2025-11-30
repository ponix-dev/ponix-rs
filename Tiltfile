allow_k8s_contexts('admin@talos')

# Ensure the ponix network exists before docker-compose runs
local_resource(
    'ponix-network',
    cmd='docker network inspect ponix >/dev/null 2>&1 || docker network create ponix',
    labels=['infrastructure'],
)

# Read BSR token from environment (managed by mise)
bsr_token = os.getenv('BSR_TOKEN', '')

# Build the Docker image with BSR authentication
# Pass the token directly as an environment variable to avoid file management issues
docker_build(
    'ponix-all-in-one:latest',
    context='.',
    dockerfile='./crates/ponix_all_in_one/Dockerfile',
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

# Run via Docker Compose (depends on network being created first)
docker_compose('./docker/docker-compose.service.yaml')

# Configure docker-compose resources with labels and dependencies
dc_resource('nats', labels=['infrastructure'], resource_deps=['ponix-network'])
dc_resource('clickhouse', labels=['infrastructure'], resource_deps=['ponix-network'])
dc_resource('postgres', labels=['infrastructure'], resource_deps=['ponix-network'])
dc_resource('otel-lgtm', labels=['observability'], resource_deps=['ponix-network'])
dc_resource('emqx', labels=['infrastructure'], resource_deps=['ponix-network'])
dc_resource('ponix-all-in-one', labels=['services'], resource_deps=['ponix-network'])
