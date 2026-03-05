"""Sound utility for task notifications."""

import os
import platform
import subprocess
import logging

logger = logging.getLogger(__name__)

def play_finish_sound():
    """Play a sound to indicate task completion.
    
    Tries various system-specific methods to play a sound.
    Fails silently if no method works.
    """
    system = platform.system()
    try:
        if system == "Darwin":  # macOS
            # Glass is a nice built-in sound
            subprocess.Popen(["afplay", "/System/Library/Sounds/Glass.aiff"], 
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        elif system == "Linux":
            # Try common Linux sound players
            players = ["paplay", "aplay", "play", "cvlc"]
            # Common locations for sounds on Linux
            sounds = [
                "/usr/share/sounds/freedesktop/stereo/complete.oga",
                "/usr/share/sounds/gnome/default/alerts/glass.ogg",
                "/usr/share/sounds/alsa/Front_Center.wav"
            ]
            
            played = False
            for player in players:
                # Check if player exists
                try:
                    if subprocess.run(["which", player], capture_output=True, shell=False).returncode == 0:
                        for sound in sounds:
                            if os.path.exists(sound):
                                if player == "cvlc":
                                    subprocess.Popen([player, "--play-and-exit", sound], 
                                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                                else:
                                    subprocess.Popen([player, sound], 
                                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                                played = True
                                break
                except Exception:
                    continue
                if played:
                    break
            
            if not played:
                # Fallback to system bell
                print("\a", end="", flush=True)
                
        elif system == "Windows":
            # Use PowerShell to play a system sound
            # We use double backslashes for the path and escape them for the shell if needed
            sound_path = "C:\\Windows\\Media\\notify.wav"
            ps_cmd = f"(New-Object Media.SoundPlayer '{sound_path}').PlaySync()"
            subprocess.Popen(["powershell", "-Command", ps_cmd], 
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        else:
            # Fallback for other systems
            print("\a", end="", flush=True)
    except Exception as e:
        # Ignore errors if sound cannot be played
        logger.debug(f"Failed to play finish sound: {e}")
        pass
