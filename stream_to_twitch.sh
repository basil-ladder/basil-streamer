BWAPI_CONFIG_AI__AI=camera.so BWAPI_CONFIG_AUTO_MENU__AUTO_MENU=SINGLE_PLAYER OPENBW_SCREEN_HEIGHT=640 xvfb-run -s "-ac -screen 0 1280x720x24" sh start_replay_viewer.sh &
sleep 1
while true; do ffmpeg -r 30 -s 1280x720 -f x11grab -draw_mouse 0 -i :99 -c:v libx264 -preset veryfast -b:v 3000k -maxrate 3000k -bufsize 4000k -pix_fmt yuv420p -an -f flv rtmp://live-fra02.twitch.tv/app/<twitch-id> ; done