CC = gcc
CFLAGS = -Wall -Werror -g

writer: main.o
	$(CC) $(CFLAGS) -o $@ $<

main.o: main.c
	$(CC) $(CFLAGS) -c $<

clean:
	rm -f writer *.o
