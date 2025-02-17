CC=g++

CFLAGS=-I.
DEPS = nbody.h

nbody.o: nbody.cpp $(DEPS)
	$(CC) -c nbody.cpp $(CFLAGS)
	
main.o: 
	$(CC) -c main.cpp $(CFLAGS)

all: nbody.o main.o 
	$(CC) -o nbodySim nbody.o main.o
	
clean:
	rm  -f *.o
	rm -f ./nbodySim
