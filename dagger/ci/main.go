package main

import (
	"context"
	"log"
	"os"
	"strings"

	"dagger/ci/internal/dagger"
)

type Ci struct{}

// Create build cache volumes
var buildkitCache = dag.CacheVolume("buildkit-cache")
var dockerCache = dag.CacheVolume("docker-cache")

func (m *Ci) Test(ctx context.Context, source *dagger.Directory) (string, error) {
	log.SetOutput(os.Stdout)
	log.SetFlags(log.Ltime | log.Lmsgprefix)

	networksTomlContent, err := m.getLocalnetImage().
		File("/workdir/localnet-data/networks.toml").
		Contents(ctx)
	if err != nil {
		return "", err
	}
	// Replace "localhost" with "localnet" in the networks.toml content
	networksTomlContent = strings.ReplaceAll(networksTomlContent, "localhost", "localnet")

	return m.codeContainer(source, networksTomlContent).
		WithServiceBinding("localnet", m.localnetService()).
		WithExec([]string{
			"sh", "-c",
			"make test",
		}).
		WithExec([]string{
			"sh", "-c",
			"find tests/cli -type f | sort | xargs -I{} sh -c 'chmod +x {} && {}'",
		}).
		Stdout(ctx)
}

func (m *Ci) getLocalnetImage() *dagger.Container {
	localnetImage := os.Getenv("LOCALNET_IMAGE")
	if localnetImage == "" {
		localnetImage = "textile/recall-localnet"
	}
	return m.getContainerWithAuth().From(localnetImage)
}

func (m *Ci) getContainerWithAuth() *dagger.Container {
	container := dag.Container().
		WithEnvVariable("DOCKER_BUILDKIT", "1").
		WithMountedCache("/root/.cache/buildkit", buildkitCache).
		WithMountedCache("/var/lib/docker", dockerCache)
	dockerUsername := os.Getenv("DOCKER_USERNAME")
	dockerPassword := os.Getenv("DOCKER_PASSWORD")
	if dockerUsername == "" || dockerPassword == "" {
		return container
	}
	dockerPasswordSecret := dag.SetSecret("DOCKER_PASSWORD", dockerPassword)
	return container.
		WithRegistryAuth("docker.io", dockerUsername, dockerPasswordSecret).
		WithSecretVariable("DOCKER_PASSWORD", dockerPasswordSecret).
		// Login to Docker so that we don't run into rate limits while pulling images from inside the localnet image
		WithExec([]string{
			"sh", "-c",
			"echo $DOCKER_PASSWORD | docker login -u " + dockerUsername + " --password-stdin",
		})
}

func (m *Ci) codeContainer(source *dagger.Directory, networksTomlContent string) *dagger.Container {
	// Create Rust-specific caches
	cargoRegistry := dag.CacheVolume("cargo-registry")
	cargoGit := dag.CacheVolume("cargo-git")
	cargoTarget := dag.CacheVolume("cargo-target")
	rustupCache := dag.CacheVolume("rustup-cache")

	container := m.getContainerWithAuth()

	// Only set up a private key if one was provided
	recallPrivateKey := os.Getenv("RECALL_PRIVATE_KEY")
	if recallPrivateKey != "" {
		container = container.WithSecretVariable("RECALL_PRIVATE_KEY", dag.SetSecret("RECALL_PRIVATE_KEY", recallPrivateKey))
	}

	return container.
		From("rust:slim-bookworm").
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
		// Rust caches and env vars
		WithMountedCache("/root/.cargo/registry", cargoRegistry).
		WithMountedCache("/root/.cargo/git", cargoGit).
		WithMountedCache("/root/.rustup", rustupCache).
		WithMountedCache("/src/target", cargoTarget).
		WithEnvVariable("CARGO_INCREMENTAL", "1").
		WithEnvVariable("CARGO_NET_RETRY", "10").
		WithEnvVariable("CARGO_NET_GIT_FETCH_WITH_CLI", "true").
		// Create the config directory and file
		WithExec([]string{
			"mkdir", "-p", "/root/.config/recall",
		}).
		WithExec([]string{
			"sh", "-c",
			"cat > /root/.config/recall/networks.toml << 'EOL'\n" + networksTomlContent + "\nEOL",
		}).
		WithDirectory("/src", source).
		WithWorkdir("/src").
		WithEnvVariable("TEST_TARGET_NETWORK_CONFIG", "/root/.config/recall/networks.toml").
		WithEnvVariable("RECALL_NETWORK", "localnet").
		WithExec([]string{
			"sh", "-c",
			"make build install",
		})
}

func (m *Ci) localnetService() *dagger.Service {
	return m.getLocalnetImage().
		WithExposedPort(8545).
		WithExposedPort(8645).
		WithExposedPort(26657).
		AsService(
			dagger.ContainerAsServiceOpts{
				InsecureRootCapabilities: true,
				NoInit:                   true,
				UseEntrypoint:            true,
			},
		).
		WithHostname("localnet")
}
