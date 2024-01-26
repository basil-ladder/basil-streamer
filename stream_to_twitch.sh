BWAPI_CONFIG_AI__AI=camera.so BWAPI_CONFIG_AUTO_MENU__AUTO_MENU=SINGLE_PLAYER OPENBW_SCREEN_HEIGHT=640 xvfb-run -s "-ac -screen 0 1280x720x24" sh setup_replay_viewer.sh &
sleep 1

while true; do ffmpeg -thread_queue_size 32768 -f x11grab -draw_mouse 0 -s 1280x720 -framerate 25 -i :99 -f pulse -ac 1 -i default -c:v libx264 -preset ultrafast -b:v 3000k -maxrate 3000k -bufsize 8000k -pix_fmt yuv420p -c:a aac -b:a 128k -ar 22050 -f flv rtmp://live-fra02.twitch.tv/app/<twitch-id> ; done


