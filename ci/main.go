package main

import (
	"context"
	"log"
	"os"

	"dagger/ci/internal/dagger"
)

type Ci struct{}

// Create build cache volumes
var buildkitCache = dag.CacheVolume("buildkit-cache")
var dockerCache = dag.CacheVolume("docker-cache")

func (m *Ci) Test(
	ctx context.Context,
	dockerUsername string,
	dockerPassword *dagger.Secret,
	source *dagger.Directory,
	testTargetNetwork string,
	recallPrivateKey *dagger.Secret,
) (string, error) {
	log.SetOutput(os.Stdout)
	log.SetFlags(log.Ltime | log.Lmsgprefix)
	os.Setenv("DOCKER_HOST", "unix:///var/run/docker.sock")

	return m.codeContainer(source, testTargetNetwork, recallPrivateKey).
		WithServiceBinding("localnet", m.localnetService(dockerUsername, dockerPassword)).
		Stdout(ctx)
}

func (m *Ci) codeContainer(
	source *dagger.Directory,
	testTargetNetwork string,
	recallPrivateKey *dagger.Secret,
) *dagger.Container {
	// Create Rust-specific caches
	cargoRegistry := dag.CacheVolume("cargo-registry")
	cargoGit := dag.CacheVolume("cargo-git")
	cargoTarget := dag.CacheVolume("cargo-target")
	rustupCache := dag.CacheVolume("rustup-cache")

	return dag.Container().
		WithEnvVariable("DOCKER_BUILDKIT", "1").
		WithMountedCache("/root/.cache/buildkit", buildkitCache).
		WithMountedCache("/var/lib/docker", dockerCache).
		From("rust:slim").
		WithExec([]string{
			"apt-get", "update",
		}).
		WithExec([]string{
			"apt-get", "install", "-y",
			"make",
			"build-essential",
			"pkg-config",
			"libssl-dev",
			"git",
		}).
		// Cargo caches and env vars
		WithMountedCache("/usr/local/cargo/registry", cargoRegistry).
		WithMountedCache("/usr/local/cargo/git", cargoGit).
		WithMountedCache("/src/target", cargoTarget).
		WithMountedCache("/root/.rustup", rustupCache).
		WithEnvVariable("CARGO_INCREMENTAL", "1").
		WithEnvVariable("CARGO_NET_RETRY", "10").
		WithEnvVariable("RUSTFLAGS", "-C target-cpu=native").
		// Create the config directory and file
		WithExec([]string{
			"mkdir", "-p", "/root/.config/recall",
		}).
		WithExec([]string{
			"sh", "-c",
			`cat > /root/.config/recall/networks.toml << 'EOL'
network = "localnet"
rpc_url = "http://localnet:26657"
evm_rpc_url = "http://localnet:8645"
object_api_url = "http://localnet:8001"
parent_evm_rpc_url = "http://localnet:8545"
EOL`,
		}).
		WithDirectory("/src", source).
		WithWorkdir("/src").
		WithEnvVariable("TEST_TARGET_NETWORK", testTargetNetwork).
		WithSecretVariable("RECALL_PRIVATE_KEY", recallPrivateKey).
		WithExec([]string{
			"sh", "-c",
			"make test",
		})
}

func (m *Ci) localnetService(dockerUsername string, dockerPassword *dagger.Secret) *dagger.Service {
	log.Println("username", dockerUsername)

	container := dag.Container().
		WithRegistryAuth("docker.io", dockerUsername, dockerPassword).
		WithEnvVariable("DOCKER_BUILDKIT", "1").
		WithMountedCache("/root/.cache/buildkit", buildkitCache).
		WithMountedCache("/var/lib/docker", dockerCache).
		From("textile/recall-localnet").
		WithSecretVariable("DOCKER_PASSWORD", dockerPassword).
		WithExec([]string{
			"sh", "-c",
			"echo $DOCKER_PASSWORD | docker login -u " + dockerUsername + " --password-stdin",
		})

	for _, port := range []int{8545, 8645, 26657} {
		container = container.WithExposedPort(
			port,
			dagger.ContainerWithExposedPortOpts{
				ExperimentalSkipHealthcheck: true,
			},
		)
	}

	return container.AsService(
		dagger.ContainerAsServiceOpts{
			InsecureRootCapabilities: true,
			NoInit:                   true,
			UseEntrypoint:            true,
		},
	)
}
