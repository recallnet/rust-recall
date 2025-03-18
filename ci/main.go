package main

import (
	"context"
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"
	"time"

	"dagger.io/dagger"
)

func main() {
	log.SetOutput(os.Stdout)
	log.SetFlags(log.Ltime | log.Lmsgprefix)
	log.Println("Starting Dagger service")

	os.Setenv("DOCKER_HOST", "unix:///var/run/docker.sock")

	// Set up signal channel before starting any services
	signalChan := make(chan os.Signal, 1)
	signal.Notify(signalChan, syscall.SIGINT, syscall.SIGTERM)

	// Create our context that we'll cancel when shutting down
	ctx, cancel := context.WithCancel(context.Background())

	log.Println("Connecting to Dagger...")
	client, err := dagger.Connect(ctx, dagger.WithLogOutput(os.Stdout))
	if err != nil {
		log.Fatalf("Failed to connect to Dagger: %v\n", err)
	}

	log.Println("Setting up service...")
	service, hostPorts := setupService(client)

	errChan := make(chan error, 1)

	log.Println("Starting service...")
	go func() {
		if err := service.Up(ctx, dagger.ServiceUpOpts{
			Ports: hostPorts,
		}); err != nil {
			if ctx.Err() != nil {
				log.Println("Service stopped due to context cancellation (normal during shutdown)")
				return
			}
			errChan <- fmt.Errorf("service failed: %w", err)
		}
	}()

	// Wait for either a signal or service error
	var sig os.Signal
	select {
	case sig = <-signalChan:
		log.Printf("Received signal: %s\n", sig)
	case err := <-errChan:
		log.Printf("Service error: %v\n", err)
	}

	// Now perform synchronous shutdown in the main thread
	log.Println("Beginning synchronous shutdown sequence...")
	cancel()

	shutdownCtx, shutdownCancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer shutdownCancel()

	log.Println("Stopping service...")
	_, err = service.Stop(
		shutdownCtx,
		dagger.ServiceStopOpts{
			Kill: true,
		},
	)
	if err != nil {
		log.Printf("Error stopping service: %v\n", err)
	} else {
		log.Println("Service stopped successfully")
	}

	log.Println("Closing Dagger client...")
	if err := client.Close(); err != nil {
		log.Printf("Error closing client: %v\n", err)
	} else {
		log.Println("Client closed successfully")
	}

	// Small delay to ensure resources are cleaned up
	time.Sleep(1 * time.Second)
	log.Println("Shutdown complete, exiting now")
}

// setupService creates and configures the container service
func setupService(client *dagger.Client) (*dagger.Service, []dagger.PortForward) {
	username := os.Getenv("DOCKER_USERNAME")
	passwordSecret := client.SetSecret("DOCKER_PASSWORD", os.Getenv("DOCKER_PASSWORD"))

	// Define ports to expose
	ports := []int{8545, 8645, 26657}

	// Create build cache volumes
	buildkitCache := client.CacheVolume("buildkit-cache")
	dockerCache := client.CacheVolume("docker-cache")

	container := client.Container().
		WithRegistryAuth("docker.io", username, passwordSecret).
		WithEnvVariable("DOCKER_BUILDKIT", "1").
		WithMountedCache("/root/.cache/buildkit", buildkitCache).
		WithMountedCache("/var/lib/docker", dockerCache).
		From("textile/recall-localnet").
		WithSecretVariable("DOCKER_PASSWORD", passwordSecret).
		WithExec([]string{
			"sh", "-c",
			"echo $DOCKER_PASSWORD | docker login -u " + username + " --password-stdin",
		})

	service := container.AsService(
		dagger.ContainerAsServiceOpts{
			InsecureRootCapabilities: true,
			NoInit:                   true,
			UseEntrypoint:            true,
		},
	)

	hostPorts := make([]dagger.PortForward, 0)
	for _, port := range ports {
		container = container.WithExposedPort(
			port,
			dagger.ContainerWithExposedPortOpts{
				ExperimentalSkipHealthcheck: true,
			},
		)
		hostPorts = append(
			hostPorts,
			dagger.PortForward{
				Backend:  port,
				Frontend: port,
				Protocol: dagger.NetworkProtocolTcp,
			})
	}

	return service, hostPorts
}
