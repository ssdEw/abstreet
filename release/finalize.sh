#!/bin/bash
# Run from ~/Downloads. Work around Github Actions annoyances.

set -e

VERSION=$1
if [ "$VERSION" == "" ]; then
	echo You forgot to pass version
	exit 1
fi

echo y | unzip abst_linux.zip
unzip abst_linux.zip
rm -f abst_linux.zip
mv abst_linux abstreet_linux_$VERSION

echo y | unzip abst_mac.zip
unzip abst_mac.zip
rm -f abst_mac.zip
mv abst_mac abstreet_mac_$VERSION

cp -Rv abstreet_mac_$VERSION abstreet_windows_$VERSION
cd abstreet_windows_$VERSION
rm -f play_abstreet.sh
cp ~/abstreet/release/play_abstreet.bat .
cd game
rm -f game
unzip ~/Downloads/game_exe.zip
rm -f ~/Downloads/game_exe.zip
cd ../..

zip -r abstreet_linux_$VERSION abstreet_linux_$VERSION
zip -r abstreet_mac_$VERSION abstreet_mac_$VERSION
zip -r abstreet_windows_$VERSION abstreet_windows_$VERSION
