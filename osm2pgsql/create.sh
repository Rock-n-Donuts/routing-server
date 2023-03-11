#/usr/bin/sh
osm2pgsql  -c -s -d osm -U osm -H db -W quebec-latest.osm.pbf
