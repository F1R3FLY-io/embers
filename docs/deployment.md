# Embers Application Setup Guide

This guide provides the necessary steps to set up and run the complete Embers application stack, which includes the f1r3fly clusters, the Embers backend, and the Embers frontend.

We recommend using Docker to run each component of the application. All required images are available from the [f1r3flyio organization on Docker Hub](https://hub.docker.com/u/f1r3flyio).

## 1. Prerequisites: f1r3fly Clusters (Mainnet & Testnet)

Before running the backend or frontend, you need to have two separate `f1r3fly` clusters up and running: one for `mainnet` and one for `testnet`.

- **Cluster Composition**: Each cluster must have a minimum of one validator node and one observer node.
- **Setup using Docker**: You can run each f1r3fly node using the [**official Docker image**](https://hub.docker.com/r/f1r3flyindustries/f1r3fly-scala-node).

## 2. Running the Backend (Embers)

The backend service for the application is called `embers`.

1.  **Set Environment Variables**: Create a file named `embers.env` to store your environment variables. This will make it easier to pass them to the Docker container.

    **`embers.env` file:**

    ```
    # Mainnet Cluster Configuration
    EMBERS__MAINNET__DEPLOY_SERVICE_URL="<deploy service url for mainnet validator>"
    EMBERS__MAINNET__PROPOSE_SERVICE_URL="<propose service url for mainnet validator>"
    EMBERS__MAINNET__READ_NODE_URL="<url to resp api of mainnet observer>"
    EMBERS__MAINNET__SERVICE_KEY="<private key of wallet with funds>"

    # Testnet Cluster Configuration
    EMBERS__TESTNET__DEPLOY_SERVICE_URL="<deploy service url for testnet validator>"
    EMBERS__TESTNET__PROPOSE_SERVICE_URL="<propose service url for testnet validator>"
    EMBERS__TESTNET__READ_NODE_URL="<url to resp api of testnet observer>"
    EMBERS__TESTNET__SERVICE_KEY="<private key of wallet with funds>"
    ```

2.  **Run the Service with Docker**: Once the environment file is created, start the `embers` backend service using Docker.

    ```bash
    # Expose the backend on port 3000 (or your desired port)
    docker run --env-file ./embers.env -p 3000:3000 f1r3flyio/embers:latest
    ```

## 3. Running the Frontend (embers-frontend)

The frontend for the application is `embers-frontend`.

1.  **Set Environment Variable**: The frontend needs the `API_URL` to connect to the `embers` backend.

2.  **Run the Application with Docker**: Launch the frontend application using the official Docker image. Remember to map the container's port to a port on your host machine to make it accessible in your browser.

    ```bash
    # Example: run frontend on port 8080, connecting to a backend on localhost:3000
    docker run -p 8080:80 \
      -e API_URL="http://<backend_public_ip>:3000" \
      f1r3flyio/embers-frontend:latest
    ```

    You can now access the frontend by navigating to `http://localhost:8080` in your web browser.
