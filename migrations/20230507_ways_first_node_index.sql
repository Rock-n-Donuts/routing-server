CREATE INDEX IF NOT EXISTS planet_osm_ways_nodes_idx 
ON public.planet_osm_ways USING gin (nodes);


CREATE INDEX IF NOT EXISTS planet_osm_ways_nodes_first_idx 
ON public.planet_osm_ways ((nodes[1]));
