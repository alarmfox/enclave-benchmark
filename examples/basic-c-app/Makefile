CC = gcc
CFLAGS = -Wall -Werror -g

build: pre-build app

app: main.o
	$(CC) $(CFLAGS) -o $@ main.o

main.o: main.c
	$(CC) $(CFLAGS) -c $<

pre-build:
	sleep 2

clean:
	rm -f app *.o
