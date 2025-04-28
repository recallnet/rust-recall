package main

import (
	"context"
	"log"
	"math/rand"
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
	// +optional
	localnetImage string,
	// +optional
	dockerUsername string,
	// +optional
	dockerPassword *dagger.Secret,
	source *dagger.Directory,
) (string, error) {
	log.SetOutput(os.Stdout)
	log.SetFlags(log.Ltime | log.Lmsgprefix)

	containerWithAuth, err := m.getContainerWithAuth(dockerUsername, dockerPassword)
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

	// Exclude the target and dagger directories from the sources
	source = source.
		WithoutDirectory(".git").
		WithoutDirectory("target").
		WithoutDirectory("dagger")
	codeContainer, err := m.codeContainer(containerWithAuth, source, networksTomlContent)
	if err != nil {
		return "", err
	}
	return codeContainer.
		WithServiceBinding("localnet", m.localnetService(localnetContainer)).
		WithExec([]string{"sh", "-c", "make lint"}).          // Lint
		WithExec([]string{"sh", "-c", "make test"}).          // Unit tests
		WithExec([]string{"sh", "-c", "make run-sdk-tests"}). // SDK integration tests
		WithExec([]string{"sh", "-c", "make run-cli-tests"}). // CLI integration tests
		WithExec([]string{"sh", "-c", "make doc"}).           // Docs
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
	dockerUsername string,
	dockerPassword *dagger.Secret,
) (*dagger.Container, error) {
	container := dag.Container().
		WithEnvVariable("DOCKER_BUILDKIT", "1").
		WithMountedCache("/root/.cache/buildkit", buildkitCache).
		WithMountedCache("/var/lib/docker", dockerCache)

	if (dockerUsername == "") || (dockerPassword == nil) {
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

	_, testAccountPrivateKey := m.getRandomTestAccount()

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
		WithEnvVariable("RECALL_NETWORK_CONFIG_FILE", "/root/.config/recall/networks.toml").
		WithEnvVariable("RECALL_NETWORK", "localnet").
		WithEnvVariable("RECALL_PRIVATE_KEY", testAccountPrivateKey).
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

func (m *Ci) getRandomTestAccount() (string, string) {
	type testAccount struct {
		address    string
		privateKey string
	}
	// The first two Anvil test accounts are intentionally excluded since they are used to submit validator IPC
	// transactions in the 2-node localnet setup used for testing. Using those accounts in tests can lead to nonce
	// clashing issues and cause unexpected failures.
	defaultTestAccounts := []testAccount{
		{
			address:    "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC",
			privateKey: "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
		},
		{
			address:    "0x90F79bf6EB2c4f870365E785982E1f101E93b906",
			privateKey: "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6",
		},
		{
			address:    "0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65",
			privateKey: "0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a",
		},
		{
			address:    "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc",
			privateKey: "0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba",
		},
		{
			address:    "0x976EA74026E726554dB657fA54763abd0C3a0aa9",
			privateKey: "0x92db14e403b83dfe3df233f83dfa3a0d7096f21ca9b0d6d6b8d88b2b4ec1564e",
		},
		{
			address:    "0x14dC79964da2C08b23698B3D3cc7Ca32193d9955",
			privateKey: "0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356",
		},
		{
			address:    "0x23618e81E3f5cdF7f54C3d65f7FBc0aBf5B21E8f",
			privateKey: "0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97",
		},
		{
			address:    "0xa0Ee7A142d267C1f36714E4a8F75612F20a79720",
			privateKey: "0x2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6",
		},
	}

	randomIndex := rand.Intn(len(defaultTestAccounts))
	randomAccount := defaultTestAccounts[randomIndex]
	return randomAccount.address, randomAccount.privateKey
}
