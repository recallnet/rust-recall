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

func (m *Ci) Test(
	ctx context.Context,
	localnetImage string,
	dockerUsername string,
	dockerPassword *dagger.Secret,
	source *dagger.Directory,
) (string, error) {
	log.SetOutput(os.Stdout)
	log.SetFlags(log.Ltime | log.Lmsgprefix)

	containerWithAuth, err := m.getContainerWithAuth(ctx, dockerUsername, dockerPassword)
	if err != nil {
		return "", err
	}
	localnetContainer, err := m.getLocalnetImage(containerWithAuth, localnetImage)
	if err != nil {
		return "", err
	}

	networksTomlContent, err := localnetContainer.
		File("/workdir/localnet-data/networks.toml").
		Contents(ctx)
	if err != nil {
		return "", err
	}
	// Replace "localhost" with "localnet" in the networks.toml content
	networksTomlContent = strings.ReplaceAll(networksTomlContent, "localhost", "localnet")

	codeContainer, err := m.codeContainer(containerWithAuth, source, networksTomlContent)
	if err != nil {
		return "", err
	}
	return codeContainer.
		WithServiceBinding("localnet", m.localnetService(localnetContainer)).
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

func (m *Ci) getLocalnetImage(
	containerWithAuth *dagger.Container,
	localnetImage string,
) (*dagger.Container, error) {
	if localnetImage == "" {
		localnetImage = "textile/recall-localnet"
	}
	return containerWithAuth.From(localnetImage), nil
}

func (m *Ci) getContainerWithAuth(
	ctx context.Context,
	dockerUsername string,
	dockerPassword *dagger.Secret,
) (*dagger.Container, error) {
	container := dag.Container().
		WithEnvVariable("DOCKER_BUILDKIT", "1").
		WithMountedCache("/root/.cache/buildkit", buildkitCache).
		WithMountedCache("/var/lib/docker", dockerCache)

	dockerPasswordText, err := dockerPassword.Plaintext(ctx)
	if err != nil {
		return nil, err
	}
	if dockerUsername == "" || dockerPasswordText == "" {
		return container, nil
	}

	return container.
		WithRegistryAuth("docker.io", dockerUsername, dockerPassword).
		WithSecretVariable("DOCKER_PASSWORD", dockerPassword).
		// Login to Docker so that we don't run into rate limits while pulling images from inside the localnet image
		WithExec([]string{
			"sh", "-c",
			"echo $DOCKER_PASSWORD | docker login -u " + dockerUsername + " --password-stdin",
		}), nil
}

func (m *Ci) codeContainer(
	containerWithAuth *dagger.Container,
	source *dagger.Directory,
	networksTomlContent string,
) (*dagger.Container, error) {
	// Create Rust-specific caches
	cargoRegistry := dag.CacheVolume("cargo-registry")
	cargoGit := dag.CacheVolume("cargo-git")
	cargoTarget := dag.CacheVolume("cargo-target")
	rustupCache := dag.CacheVolume("rustup-cache")

	return containerWithAuth.
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
			"jq",
			"bc",
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
		}), nil
}

func (m *Ci) localnetService(localnetContainer *dagger.Container) *dagger.Service {
	return localnetContainer.
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
