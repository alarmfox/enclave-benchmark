#!/bin/sh

# Function to display help message
show_help() {
    echo "Usage: $0 -d <directory> -u <user>"
    echo "  -d, --directory   Specify the directory to create"
    echo "  -u, --user        Specify the user for ownership"
    echo "  -h, --help        Display this help message"
}

# Parse command line arguments
while [ $# -gt 0 ]; do
    case "$1" in
        -d|--directory)
            DIRECTORY="$2"
            shift 2
            ;;
        -u|--user)
            USER="$2"
            shift 2
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
done

# Check if both directory and user are provided
if [ -z "$DIRECTORY" ] || [ -z "$USER" ]; then
    echo "Error: Both directory and user must be specified."
    show_help
    exit 1
fi

# Create the directory and set ownership
mkdir -p "$DIRECTORY"
chown -R "$USER:$USER" "$DIRECTORY"

