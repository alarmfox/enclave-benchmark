CC = gcc
CFLAGS = -Wall -Werror -g

app: main.o
	$(CC) $(CFLAGS) -o app $<

main.o: main.c
	$(CC) $(CFLAGS) -c $<

clean:
	rm -f app *.o
