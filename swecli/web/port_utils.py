"""Utility functions for finding available ports."""

import socket
from typing import Optional


def is_port_available(host: str, port: int) -> bool:
    """Check if a port is available.
    
    Args:
        host: Host address to check
        port: Port number to check
        
    Returns:
        True if port is available, False otherwise
    """
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            sock.bind((host, port))
            return True
    except OSError:
        return False


def find_available_port(host: str, preferred_port: int, max_attempts: int = 10) -> Optional[int]:
    """Find an available port starting from preferred_port.
    
    Args:
        host: Host address to bind to
        preferred_port: Preferred port to try first
        max_attempts: Maximum number of ports to try
        
    Returns:
        Available port number, or None if no port found
    """
    for offset in range(max_attempts):
        port = preferred_port + offset
        if is_port_available(host, port):
            return port
    return None
