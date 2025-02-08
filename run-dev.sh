#!/bin/bash

# Open the first tab and run the database proxy script
gnome-terminal --tab --title="Database Proxy" -- bash -c "./database/proxy.sh; exec bash"

# Open the second tab and connect to Redis
gnome-terminal --tab --title="Redis Connect" -- bash -c "fly redis connect; exec bash"

# Open the third tab and start the app
gnome-terminal --tab --title="PNPM Dev" -- bash -c "pnpm dev:app; exec bash"

# Open the fourth tab and navigate to the app directory
gnome-terminal --tab --title="App Directory" -- bash -c "cd apps/app; exec bash"