ALTER TABLE public.ways_length ADD first_node int8;
ALTER TABLE public.ways_length ADD last_node int8;

CREATE INDEX IF NOT EXISTS ways_length_first_node_idx ON public.ways_length (first_node);
CREATE INDEX IF NOT EXISTS ways_length_last_node_idx ON public.ways_length (last_node);