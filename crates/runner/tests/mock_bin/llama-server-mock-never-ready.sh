#!/bin/sh
# Mock llama-server that stays alive but never serves /health: for
# exercising the ready-polling timeout path.
exec sleep 60
