#!/bin/bash

helpFunction()
{
   echo ""
   echo "Usage: $0 -r ripetirions -n bodies -p program"
   echo -e "\t-r Number of ripetitions"
   echo -e "\t-n Number of bodies - size of the problem"
   echo -e "\t-p Path to nbody program"
   exit 1 # Exit script after printing help
}

while getopts "r:n:p:" opt
do
   case "$opt" in
      r ) parameterA="$OPTARG" ;;
      n ) parameterB="$OPTARG" ;;
      p ) programPath="$OPTARG" ;; 
      ? ) helpFunction ;; # Print helpFunction in case parameter is non-existent
   esac
done


echo $parameterA  $parameterB $programPath
# Print helpFunction in case parameters are empty
if [ -z "$parameterA" ] || [ -z "$parameterB" ] || [ -z "$programPath" ] 
then
   echo "Some or all of the parameters are empty";
   helpFunction
fi

for (( counter=$parameterA; counter>0; counter-- ))
do
$programPath $parameterB
done
printf "\n"

