#!/bin/bash

fly proxy 9432:5432 -a blue-surf-3106 --bind-addr 127.0.0.1