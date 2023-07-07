CREATE TABLE if not EXISTS public.ways_length (
	ways_id int8 NULL,
	length int8 NULL,
	CONSTRAINT ways_length_fk FOREIGN KEY (ways_id) REFERENCES public.planet_osm_ways(id)
);
