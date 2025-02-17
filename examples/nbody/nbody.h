#ifndef NBODY_H
#define NBODY_H

// Parameters
const int DIM = 2;          // dimensions
const double EPS = 1e-4;    // smoothing parameter

// Function prototypes
inline double sqr(double);

struct Particle{
    double m;           // mass
    double x[DIM];      // position
    double v[DIM];      // velocity
    double F[DIM];      // force 
    double F_old[DIM];  // force past time step
};

// Nbody class 
class Nbody {
    private:
        int step = 0;
        double t = 0;
        const int n;         // number of particles
        const double dt;           // step size
        const double t_max;        // max simulation time
        Particle *p = new Particle[n]; // allocate memory 
        void init_data();
    public:
        ~Nbody();
        Nbody(int n_, double dt_, double t_max_);
        inline void print_parameter() const;
        inline void print_data() const;
        inline void write_data(int step) const;
        void timeIntegration();
        void comp_force();
        void force(Particle*, Particle*);
        void comp_position();
        void comp_velocity();
        void update_position(Particle*);
        void update_velocity(Particle*);
};

#endif