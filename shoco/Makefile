FLAGS=$(CFLAGS) -std=c99 -O3 -Wall
SOURCES=shoco.c
OBJECTS=$(SOURCES:.c=.o)
HEADERS=shoco.h shoco_model.h
GENERATOR=generate_compressor_model.py

.PHONY: all
all: shoco

shoco: shoco-bin.o $(OBJECTS) $(HEADERS)
	$(CC) $(LDFLAGS) $(OBJECTS) $< -o $@

test_input: test_input.o $(OBJECTS) $(HEADERS)
	$(CC) $(LDFLAGS) $(OBJECTS) $< -o $@

$(OBJECTS): %.o: %.c $(HEADERS)
	$(CC) $(FLAGS) $< -c

.PHONY: models
models: $(TABLES)

.PHONY: check
check: tests

tests: tests.o $(OBJECTS) $(HEADERS)
	$(CC) $(LDFLAGS) $(OBJECTS) $< -o $@
	./tests

.PHONY: clean
clean:
	rm *.o
