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
			"curl",
		}).
		// Rust caches and env vars
		WithMountedCache("/root/.cargo/registry", cargoRegistry).
		WithMountedCache("/root/.cargo/git", cargoGit).
		WithMountedCache("/src/target", cargoTarget).
		WithMountedCache("/root/.rustup", rustupCache).
		WithEnvVariable("CARGO_INCREMENTAL", "1").
		WithEnvVariable("CARGO_NET_RETRY", "10").
		WithEnvVariable("CARGO_NET_GIT_FETCH_WITH_CLI", "true").
		WithEnvVariable("RUSTFLAGS", "-C target-cpu=native").
		WithExec([]string{"bash", "-c",
			"curl -o- https://sh.rustup.rs | sh -s -- -y --default-toolchain stable",
		}).
		// Create the config directory and file
		WithExec([]string{
			"mkdir", "-p", "/root/.config/recall",
		}).
		WithExec([]string{
			"sh", "-c",
			`cat > /root/.config/recall/networks.toml << 'EOL'
[localnet.subnet_config]
chain_id = 248163216
subnet_id = "/r31337/t410f6gbdxrbehnaeeo4mrq7wc5hgq6smnefys4qanwi"
rpc_url = "http://localnet:26657/"
object_api_url = "http://localnet:8001/"
evm_rpc_url = "http://localnet:8645/"
evm_gateway_address = "0x77aa40b105843728088c0132e43fc44348881da8"
evm_registry_address = "0x74539671a1d2f1c8f200826baba665179f53a1b7"

[localnet.parent_config]
evm_rpc_url = "http://localnet:8545/"
evm_gateway_address = "0x9a676e781a523b5d0c0e43731313a708cb607508"
evm_registry_address = "0x322813fd9a801c5507c9de605d63cea4f2ce6c44"
evm_supply_source_address = "0x4a679253410272dd5232b3ff7cf5dbb88f295319"
EOL`,
		}).
		WithDirectory("/src", source).
		WithWorkdir("/src").
		WithEnvVariable("TEST_TARGET_NETWORK", testTargetNetwork).
		WithSecretVariable("RECALL_PRIVATE_KEY", recallPrivateKey).
		WithExec([]string{
			"sh", "-c",
			"make build install",
		}).
		WithExec([]string{
			"sh", "-c",
			"recall --network localnet account deposit 1",
		})
}

func (m *Ci) localnetService(dockerUsername string, dockerPassword *dagger.Secret) *dagger.Service {
	return dag.Container().
		WithRegistryAuth("docker.io", dockerUsername, dockerPassword).
		WithEnvVariable("DOCKER_BUILDKIT", "1").
		WithMountedCache("/root/.cache/buildkit", buildkitCache).
		WithMountedCache("/var/lib/docker", dockerCache).
		From("textile/recall-localnet").
		WithSecretVariable("DOCKER_PASSWORD", dockerPassword).
		WithExec([]string{
			"sh", "-c",
			"echo $DOCKER_PASSWORD | docker login -u " + dockerUsername + " --password-stdin",
		}).
		WithExposedPort(8545).
		AsService(
			dagger.ContainerAsServiceOpts{
				InsecureRootCapabilities: true,
				NoInit:                   true,
				UseEntrypoint:            true,
			},
		).
		WithHostname("localnet")
}
