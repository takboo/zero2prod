#!/usr/bin/env bash
set -x
set -eo pipefail

# Load environment variables from .env file in the project root
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
ENV_FILE="$SCRIPT_DIR/.env"

if [ -f "$ENV_FILE" ]; then
  echo "Loading environment variables from $ENV_FILE"
  set -a # Automatically export all variables defined in the sourced file
  # shellcheck disable=SC1091
  source "$ENV_FILE"
  set +a # Disable auto-export
else
  # It's not an error if .env is not found, variables might be set globally
  # or docker-compose might use its own .env handling or defaults.
  echo "INFO: $ENV_FILE not found. Relying on environment variables already set or defaults."
fi

if ! [ -x "$(command -v psql)" ]; then
  echo >&2 "Error: psql is not installed."
  exit 1
fi

if ! [ -x "$(command -v sqlx)" ]; then
  echo >&2 "Error: sqlx is not installed."
  echo >&2 "Use:"
  echo >&2 "    cargo install --version=0.6.0 sqlx-cli --no-default-features --features postgres"
  echo >&2 "to install it."
  exit 1
fi

# Allow to skip Docker if a dockerized Postgres database is already running
if [[ -z "${SKIP_DOCKER}" ]]
then
  docker compose -f  $SCRIPT_DIR/docker-compose.yml up -d
fi

# Keep pinging Postgres until it's ready to accept commands from the host.
# This ensures that sqlx (running on the host) can connect.
until PGPASSWORD="${DB_PASSWORD}" psql -h "localhost" -U "${DB_USER}" -p "${DB_PORT}" -d "postgres" -c '\q'; do
  >&2 echo "Postgres is still unavailable - sleeping"
  sleep 1
done

>&2 echo "Postgres is up and running on port ${DB_PORT} - running migrations now!"

export DATABASE_URL=postgres://${DB_USER}:${DB_PASSWORD}@localhost:${DB_PORT}/${DB_NAME}
sqlx database create
sqlx migrate run

>&2 echo "Postgres has been migrated, ready to go!"