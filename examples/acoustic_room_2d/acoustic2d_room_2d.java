/*
 * acoustic2d_cylcscatter.java
 */

import com.comsol.model.*;
import com.comsol.model.util.*;

/** Model exported on Oct 14 2026, 10:33 by COMSOL 6.2.0.290. */
public class acoustic2d_cylcscatter {

  public static Model run() {
    Model model = ModelUtil.create("Model");

    model.modelPath("C:\\Users\\jense\\Documents\\codes\\FullMeta\\examples\\model-2");

    model.component().create("comp1", true);

    model.component("comp1").geom().create("geom1", 2);

    model.component("comp1").mesh().create("mesh1");

    model.component("comp1").physics().create("acpr", "PressureAcoustics", "geom1");

    model.study().create("std1");
    model.study("std1").create("freq", "Frequency");
    model.study("std1").feature("freq").setSolveFor("/physics/acpr", true);

    model.component("comp1").geom("geom1").create("r1", "Rectangle");
    model.component("comp1").geom("geom1").feature("r1").set("size", new double[]{1, 0.5});
    model.component("comp1").geom("geom1").run("r1");
    model.component("comp1").geom("geom1").run();

    model.component("comp1").physics("acpr").create("nvel1", "NormalVelocity", 1);

    model.study().create("std2");
    model.study("std2").create("eig", "Eigenfrequency");
    model.study("std2").feature("eig").set("chkeigregion", true);
    model.study("std2").feature("eig").set("conrad", "1");
    model.study("std2").feature("eig").set("conradynhm", "1");
    model.study("std2").feature("eig").set("storefact", false);
    model.study("std2").feature("eig").set("linpsolnum", "auto");
    model.study("std2").feature("eig").set("solnum", "auto");
    model.study("std2").feature("eig").set("notsolnum", "auto");
    model.study("std2").feature("eig").set("outputmap", new String[]{});
    model.study("std2").feature("eig").set("ngenAUX", "1");
    model.study("std2").feature("eig").set("goalngenAUX", "1");
    model.study("std2").feature("eig").set("ngenAUX", "1");
    model.study("std2").feature("eig").set("goalngenAUX", "1");
    model.study("std2").feature("eig").setSolveFor("/physics/acpr", true);

    model.sol().create("sol1");
    model.sol("sol1").study("std2");
    model.sol("sol1").attach("std2");

    model.component("comp1").mesh("mesh1").automatic(false);
    model.component("comp1").mesh("mesh1").feature("size").set("custom", true);
    model.component("comp1").mesh("mesh1").run("size1");

    model.component("comp1").material().create("mat1", "Common");
    model.component("comp1").material("mat1").propertyGroup("def").func().create("eta", "Piecewise");
    model.component("comp1").material("mat1").propertyGroup("def").func().create("Cp", "Piecewise");
    model.component("comp1").material("mat1").propertyGroup("def").func().create("rho", "Analytic");
    model.component("comp1").material("mat1").propertyGroup("def").func().create("k", "Piecewise");
    model.component("comp1").material("mat1").propertyGroup("def").func().create("cs", "Analytic");
    model.component("comp1").material("mat1").propertyGroup("def").func().create("an1", "Analytic");
    model.component("comp1").material("mat1").propertyGroup("def").func().create("an2", "Analytic");
    model.component("comp1").material("mat1").propertyGroup().create("RefractiveIndex", "Refractive index");
    model.component("comp1").material("mat1").propertyGroup().create("NonlinearModel", "Nonlinear model");
    model.component("comp1").material("mat1").propertyGroup().create("idealGas", "Ideal gas");
    model.component("comp1").material("mat1").propertyGroup("idealGas").func().create("Cp", "Piecewise");
    model.component("comp1").material("mat1").label("Air");
    model.component("comp1").material("mat1").set("family", "air");
    model.component("comp1").material("mat1").propertyGroup("def").func("eta").set("arg", "T");
    model.component("comp1").material("mat1").propertyGroup("def").func("eta")
         .set("pieces", new String[][]{{"200.0", "1600.0", "-8.38278E-7+8.35717342E-8*T^1-7.69429583E-11*T^2+4.6437266E-14*T^3-1.06585607E-17*T^4"}});
    model.component("comp1").material("mat1").propertyGroup("def").func("eta").set("argunit", "K");
    model.component("comp1").material("mat1").propertyGroup("def").func("eta").set("fununit", "Pa*s");
    model.component("comp1").material("mat1").propertyGroup("def").func("Cp").set("arg", "T");
    model.component("comp1").material("mat1").propertyGroup("def").func("Cp")
         .set("pieces", new String[][]{{"200.0", "1600.0", "1047.63657-0.372589265*T^1+9.45304214E-4*T^2-6.02409443E-7*T^3+1.2858961E-10*T^4"}});
    model.component("comp1").material("mat1").propertyGroup("def").func("Cp").set("argunit", "K");
    model.component("comp1").material("mat1").propertyGroup("def").func("Cp").set("fununit", "J/(kg*K)");
    model.component("comp1").material("mat1").propertyGroup("def").func("rho")
         .set("expr", "pA*0.02897/R_const[K*mol/J]/T");
    model.component("comp1").material("mat1").propertyGroup("def").func("rho").set("args", new String[]{"pA", "T"});
    model.component("comp1").material("mat1").propertyGroup("def").func("rho").set("fununit", "kg/m^3");
    model.component("comp1").material("mat1").propertyGroup("def").func("rho")
         .set("argunit", new String[]{"Pa", "K"});
    model.component("comp1").material("mat1").propertyGroup("def").func("rho")
         .set("plotaxis", new String[]{"off", "on"});
    model.component("comp1").material("mat1").propertyGroup("def").func("rho")
         .set("plotfixedvalue", new String[]{"101325", "273.15"});
    model.component("comp1").material("mat1").propertyGroup("def").func("rho")
         .set("plotargs", new String[][]{{"pA", "101325", "101325"}, {"T", "273.15", "293.15"}});
    model.component("comp1").material("mat1").propertyGroup("def").func("k").set("arg", "T");
    model.component("comp1").material("mat1").propertyGroup("def").func("k")
         .set("pieces", new String[][]{{"200.0", "1600.0", "-0.00227583562+1.15480022E-4*T^1-7.90252856E-8*T^2+4.11702505E-11*T^3-7.43864331E-15*T^4"}});
    model.component("comp1").material("mat1").propertyGroup("def").func("k").set("argunit", "K");
    model.component("comp1").material("mat1").propertyGroup("def").func("k").set("fununit", "W/(m*K)");
    model.component("comp1").material("mat1").propertyGroup("def").func("cs")
         .set("expr", "sqrt(1.4*R_const[K*mol/J]/0.02897*T)");
    model.component("comp1").material("mat1").propertyGroup("def").func("cs").set("args", new String[]{"T"});
    model.component("comp1").material("mat1").propertyGroup("def").func("cs").set("fununit", "m/s");
    model.component("comp1").material("mat1").propertyGroup("def").func("cs").set("argunit", new String[]{"K"});
    model.component("comp1").material("mat1").propertyGroup("def").func("cs")
         .set("plotfixedvalue", new String[]{"273.15"});
    model.component("comp1").material("mat1").propertyGroup("def").func("cs")
         .set("plotargs", new String[][]{{"T", "273.15", "373.15"}});
    model.component("comp1").material("mat1").propertyGroup("def").func("an1").set("funcname", "alpha_p");
    model.component("comp1").material("mat1").propertyGroup("def").func("an1")
         .set("expr", "-1/rho(pA,T)*d(rho(pA,T),T)");
    model.component("comp1").material("mat1").propertyGroup("def").func("an1").set("args", new String[]{"pA", "T"});
    model.component("comp1").material("mat1").propertyGroup("def").func("an1").set("fununit", "1/K");
    model.component("comp1").material("mat1").propertyGroup("def").func("an1")
         .set("argunit", new String[]{"Pa", "K"});
    model.component("comp1").material("mat1").propertyGroup("def").func("an1")
         .set("plotaxis", new String[]{"off", "on"});
    model.component("comp1").material("mat1").propertyGroup("def").func("an1")
         .set("plotfixedvalue", new String[]{"101325", "273.15"});
    model.component("comp1").material("mat1").propertyGroup("def").func("an1")
         .set("plotargs", new String[][]{{"pA", "101325", "101325"}, {"T", "273.15", "373.15"}});
    model.component("comp1").material("mat1").propertyGroup("def").func("an2").set("funcname", "muB");
    model.component("comp1").material("mat1").propertyGroup("def").func("an2").set("expr", "0.6*eta(T)");
    model.component("comp1").material("mat1").propertyGroup("def").func("an2").set("args", new String[]{"T"});
    model.component("comp1").material("mat1").propertyGroup("def").func("an2").set("fununit", "Pa*s");
    model.component("comp1").material("mat1").propertyGroup("def").func("an2").set("argunit", new String[]{"K"});
    model.component("comp1").material("mat1").propertyGroup("def").func("an2")
         .set("plotfixedvalue", new String[]{"200"});
    model.component("comp1").material("mat1").propertyGroup("def").func("an2")
         .set("plotargs", new String[][]{{"T", "200", "1600"}});
    model.component("comp1").material("mat1").propertyGroup("def").set("thermalexpansioncoefficient", "");
    model.component("comp1").material("mat1").propertyGroup("def").set("molarmass", "");
    model.component("comp1").material("mat1").propertyGroup("def").set("bulkviscosity", "");
    model.component("comp1").material("mat1").propertyGroup("def")
         .set("thermalexpansioncoefficient", new String[]{"alpha_p(pA,T)", "0", "0", "0", "alpha_p(pA,T)", "0", "0", "0", "alpha_p(pA,T)"});
    model.component("comp1").material("mat1").propertyGroup("def").set("molarmass", "0.02897[kg/mol]");
    model.component("comp1").material("mat1").propertyGroup("def").set("bulkviscosity", "muB(T)");
    model.component("comp1").material("mat1").propertyGroup("def")
         .set("relpermeability", new String[]{"1", "0", "0", "0", "1", "0", "0", "0", "1"});
    model.component("comp1").material("mat1").propertyGroup("def")
         .set("relpermittivity", new String[]{"1", "0", "0", "0", "1", "0", "0", "0", "1"});
    model.component("comp1").material("mat1").propertyGroup("def").set("dynamicviscosity", "eta(T)");
    model.component("comp1").material("mat1").propertyGroup("def").set("ratioofspecificheat", "1.4");
    model.component("comp1").material("mat1").propertyGroup("def")
         .set("electricconductivity", new String[]{"0[S/m]", "0", "0", "0", "0[S/m]", "0", "0", "0", "0[S/m]"});
    model.component("comp1").material("mat1").propertyGroup("def").set("heatcapacity", "Cp(T)");
    model.component("comp1").material("mat1").propertyGroup("def").set("density", "rho(pA,T)");
    model.component("comp1").material("mat1").propertyGroup("def")
         .set("thermalconductivity", new String[]{"k(T)", "0", "0", "0", "k(T)", "0", "0", "0", "k(T)"});
    model.component("comp1").material("mat1").propertyGroup("def").set("soundspeed", "cs(T)");
    model.component("comp1").material("mat1").propertyGroup("def").addInput("temperature");
    model.component("comp1").material("mat1").propertyGroup("def").addInput("pressure");
    model.component("comp1").material("mat1").propertyGroup("RefractiveIndex")
         .set("n", new String[]{"1", "0", "0", "0", "1", "0", "0", "0", "1"});
    model.component("comp1").material("mat1").propertyGroup("NonlinearModel").set("BA", "def.gamma-1");
    model.component("comp1").material("mat1").propertyGroup("idealGas").func("Cp").label("Piecewise 2");
    model.component("comp1").material("mat1").propertyGroup("idealGas").func("Cp").set("arg", "T");
    model.component("comp1").material("mat1").propertyGroup("idealGas").func("Cp")
         .set("pieces", new String[][]{{"200.0", "1600.0", "1047.63657-0.372589265*T^1+9.45304214E-4*T^2-6.02409443E-7*T^3+1.2858961E-10*T^4"}});
    model.component("comp1").material("mat1").propertyGroup("idealGas").func("Cp").set("argunit", "K");
    model.component("comp1").material("mat1").propertyGroup("idealGas").func("Cp").set("fununit", "J/(kg*K)");
    model.component("comp1").material("mat1").propertyGroup("idealGas").set("Rs", "R_const/Mn");
    model.component("comp1").material("mat1").propertyGroup("idealGas").set("heatcapacity", "Cp(T)");
    model.component("comp1").material("mat1").propertyGroup("idealGas").set("ratioofspecificheat", "1.4");
    model.component("comp1").material("mat1").propertyGroup("idealGas").set("molarmass", "0.02897");
    model.component("comp1").material("mat1").propertyGroup("idealGas").addInput("temperature");
    model.component("comp1").material("mat1").propertyGroup("idealGas").addInput("pressure");
    model.component("comp1").material("mat1").materialType("nonSolid");

    model.component("comp1").mesh("mesh1").feature().remove("se1");
    model.component("comp1").mesh("mesh1").run();

    model.sol("sol1").study("std2");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std2");
    model.sol("sol1").feature("st1").set("studystep", "eig");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "eig");
    model.sol("sol1").create("e1", "Eigenvalue");
    model.sol("sol1").feature("e1").set("neigs", 6);
    model.sol("sol1").feature("e1").set("shift", "0");
    model.sol("sol1").feature("e1").set("rtol", 1.0E-6);
    model.sol("sol1").feature("e1").set("transform", "none");
    model.sol("sol1").feature("e1").set("eigref", "100");
    model.sol("sol1").feature("e1").set("eigvfunscale", "average");
    model.sol("sol1").feature("e1").set("control", "eig");
    model.sol("sol1").feature("e1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("e1").feature("aDef").set("cachepattern", true);
    model.sol("sol1").feature("e1").feature("aDef").set("matherr", true);
    model.sol("sol1").feature("e1").feature("aDef").set("blocksizeactive", false);
    model.sol("sol1").feature("e1").feature("aDef").set("nullfun", "auto");
    model.sol("sol1").attach("std2");
    model.sol("sol1").runAll();

    model.result().create("pg1", "PlotGroup2D");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").setIndex("looplevel", 1, 0);
    model.result("pg1").create("surf1", "Surface");
    model.result("pg1").feature("surf1").set("expr", new String[]{"acpr.p_t"});
    model.result("pg1").feature("surf1").set("colortable", "Wave");
    model.result("pg1").feature("surf1").set("colorscalemode", "linearsymmetric");
    model.result("pg1").set("showlegendsunit", true);
    model.result("pg1").feature("surf1").set("colortable", "WaveLight");
    model.result("pg1").create("con1", "Contour");
    model.result("pg1").feature("con1").set("expr", new String[]{"acpr.p_t"});
    model.result("pg1").feature("con1").set("colortable", "Wave");
    model.result("pg1").feature("con1").set("colorscalemode", "linearsymmetric");
    model.result("pg1").feature("con1").set("colorlegend", false);
    model.result("pg1").label("Acoustic Pressure (acpr)");
    model.result().create("pg2", "PlotGroup2D");
    model.result("pg2").set("data", "dset1");
    model.result("pg2").setIndex("looplevel", 1, 0);
    model.result("pg2").create("surf1", "Surface");
    model.result("pg2").feature("surf1").set("expr", new String[]{"acpr.Lp_t"});
    model.result("pg2").feature("surf1").set("colortable", "Rainbow");
    model.result("pg2").feature("surf1").set("colorscalemode", "linear");
    model.result("pg2").set("showlegendsunit", true);
    model.result("pg2").label("Sound Pressure Level (acpr)");
    model.result().evaluationGroup().create("std2EvgFrq", "EvaluationGroup");
    model.result().evaluationGroup("std2EvgFrq").set("defaultPlotID", "eigenfrequenciesTable_acpr");
    model.result().evaluationGroup("std2EvgFrq").set("data", "dset1");
    model.result().evaluationGroup("std2EvgFrq").label("Eigenfrequencies (Study 2)");
    model.result().evaluationGroup("std2EvgFrq").create("gev1", "EvalGlobal");
    model.result().evaluationGroup("std2EvgFrq").feature("gev1").setIndex("expr", "2*pi*freq", 0);
    model.result().evaluationGroup("std2EvgFrq").feature("gev1").setIndex("unit", "rad/s", 0);
    model.result().evaluationGroup("std2EvgFrq").feature("gev1").setIndex("descr", "Angular frequency", 0);
    model.result().evaluationGroup("std2EvgFrq").feature("gev1").setIndex("expr", "imag(freq)/abs(freq)", 1);
    model.result().evaluationGroup("std2EvgFrq").feature("gev1").setIndex("unit", "1", 1);
    model.result().evaluationGroup("std2EvgFrq").feature("gev1").setIndex("descr", "Damping ratio", 1);
    model.result().evaluationGroup("std2EvgFrq").feature("gev1").setIndex("expr", "abs(freq)/imag(freq)/2", 2);
    model.result().evaluationGroup("std2EvgFrq").feature("gev1").setIndex("unit", "1", 2);
    model.result().evaluationGroup("std2EvgFrq").feature("gev1").setIndex("descr", "Quality factor", 2);
    model.result().evaluationGroup("std2EvgFrq").run();
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new int[]{2});
    model.result("pg1").run();

    model.label("comsol-acoustic-2d-reference.mph");

    model.result("pg1").run();

    model.study().remove("std2");

    model.component("comp1").physics("acpr").create("port1", "Port", 1);
    model.component("comp1").physics("acpr").feature("port1").selection().set(1);

    model.component("comp1").material().remove("mat1");
    model.component("comp1").material().create("mat1", "Common");
    model.component("comp1").material("mat1").label("air");
    model.component("comp1").material("mat1").propertyGroup("def").set("density", new String[]{"1.225"});
    model.component("comp1").material("mat1").propertyGroup("def").set("soundspeed", new String[]{"343"});

    model.param().set("c0", "343[m/s]");
    model.param().set("rho0", "1.225[kg/m^3]");

    model.component("comp1").material("mat1").propertyGroup("def").set("soundspeed", new String[]{"c0"});
    model.component("comp1").material("mat1").propertyGroup("def").set("density", new String[]{"rho0"});

    model.component("comp1").physics("acpr").feature().remove("nvel1");
    model.component("comp1").physics("acpr").feature("port1").set("pamp", 1);
    model.component("comp1").physics("acpr").create("port2", "Port", 1);
    model.component("comp1").physics("acpr").feature("port2").selection().set(4);

    model.param().set("freq0", "1000[Hz]");
    model.param().set("k0", "2*pi*freq0/c0");

    model.component("comp1").physics("acpr").feature("port1").set("p", 1);
    model.component("comp1").physics("acpr").feature("port1").set("kn", "k0");
    model.component("comp1").physics("acpr").feature("port2").set("p", 1);
    model.component("comp1").physics("acpr").feature("port2").set("kn", "k0");

    model.component("comp1").mesh("mesh1").feature().remove("size1");
    model.component("comp1").mesh("mesh1").feature("ftri1").create("size1", "Size");
    model.component("comp1").mesh("mesh1").feature("ftri1").feature("size1").set("custom", true);
    model.component("comp1").mesh("mesh1").run("ftri1");
    model.component("comp1").mesh("mesh1").feature("ftri1").feature("size1").set("hmaxactive", true);
    model.component("comp1").mesh("mesh1").feature("ftri1").feature("size1").set("hmax", 0.03);
    model.component("comp1").mesh("mesh1").run("ftri1");

    model.sol().create("sol1");
    model.sol("sol1").study("std1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").set("stol", 0.001);
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{""});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "no");
    model.sol("sol1").feature("s1").feature("p1").set("pdistrib", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "Default");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("linpmethod", "sol");
    model.sol("sol1").feature("s1").set("linpsol", "zero");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").feature("aDef").set("cachepattern", true);
    model.sol("sol1").feature("s1").feature("aDef").set("matherr", true);
    model.sol("sol1").feature("s1").feature("aDef").set("blocksizeactive", false);
    model.sol("sol1").feature("s1").feature("aDef").set("nullfun", "auto");
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.result().create("pg1", "PlotGroup2D");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").setIndex("looplevel", 1, 0);
    model.result("pg1").create("surf1", "Surface");
    model.result("pg1").feature("surf1").set("expr", new String[]{"acpr.p_t"});
    model.result("pg1").feature("surf1").set("colortable", "Wave");
    model.result("pg1").feature("surf1").set("colorscalemode", "linearsymmetric");
    model.result("pg1").set("showlegendsunit", true);
    model.result("pg1").label("Acoustic Pressure (acpr)");
    model.result().create("pg2", "PlotGroup2D");
    model.result("pg2").set("data", "dset1");
    model.result("pg2").setIndex("looplevel", 1, 0);
    model.result("pg2").create("surf1", "Surface");
    model.result("pg2").feature("surf1").set("expr", new String[]{"acpr.Lp_t"});
    model.result("pg2").feature("surf1").set("colortable", "Rainbow");
    model.result("pg2").feature("surf1").set("colorscalemode", "linear");
    model.result("pg2").set("showlegendsunit", true);
    model.result("pg2").label("Sound Pressure Level (acpr)");
    model.result().remove("pg2");
    model.result().remove("pg1");

    model.study("std1").feature("freq").set("plist", "freq0");

    model.sol("sol1").study("std1");
    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").set("stol", 0.001);
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "no");
    model.sol("sol1").feature("s1").feature("p1").set("pdistrib", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "Default");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("linpmethod", "sol");
    model.sol("sol1").feature("s1").set("linpsol", "zero");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").feature("aDef").set("cachepattern", true);
    model.sol("sol1").feature("s1").feature("aDef").set("matherr", true);
    model.sol("sol1").feature("s1").feature("aDef").set("blocksizeactive", false);
    model.sol("sol1").feature("s1").feature("aDef").set("nullfun", "auto");
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result().create("pg1", "PlotGroup2D");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").setIndex("looplevel", 1, 0);
    model.result("pg1").create("surf1", "Surface");
    model.result("pg1").feature("surf1").set("expr", new String[]{"acpr.p_t"});
    model.result("pg1").feature("surf1").set("colortable", "Wave");
    model.result("pg1").feature("surf1").set("colorscalemode", "linearsymmetric");
    model.result("pg1").set("showlegendsunit", true);
    model.result("pg1").label("Acoustic Pressure (acpr)");
    model.result().create("pg2", "PlotGroup2D");
    model.result("pg2").set("data", "dset1");
    model.result("pg2").setIndex("looplevel", 1, 0);
    model.result("pg2").create("surf1", "Surface");
    model.result("pg2").feature("surf1").set("expr", new String[]{"acpr.Lp_t"});
    model.result("pg2").feature("surf1").set("colortable", "Rainbow");
    model.result("pg2").feature("surf1").set("colorscalemode", "linear");
    model.result("pg2").set("showlegendsunit", true);
    model.result("pg2").label("Sound Pressure Level (acpr)");
    model.result("pg1").run();

    model.component("comp1").geom("geom1").run("r1");
    model.component("comp1").geom("geom1").create("c1", "Circle");
    model.component("comp1").geom("geom1").feature("r1").set("base", "center");
    model.component("comp1").geom("geom1").run("r1");
    model.component("comp1").geom("geom1").feature("c1").set("r", 0.3);
    model.component("comp1").geom("geom1").run("c1");
    model.component("comp1").geom("geom1").feature("c1").set("r", 0.1);
    model.component("comp1").geom("geom1").run("c1");
    model.component("comp1").geom("geom1").run();

    model.component("comp1").material().create("mat2", "Common");
    model.component("comp1").material("mat2").label("copper");
    model.component("comp1").material("mat2").selection().set(2);
    model.component("comp1").material("mat1").selection().set(1);
    model.component("comp1").material("mat2").propertyGroup("def").set("density", new String[]{"8960"});
    model.component("comp1").material("mat2").propertyGroup("def").set("soundspeed", new String[]{"4760"});

    model.sol("sol1").study("std1");
    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").set("stol", 0.001);
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "no");
    model.sol("sol1").feature("s1").feature("p1").set("pdistrib", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("linpmethod", "sol");
    model.sol("sol1").feature("s1").set("linpsol", "zero");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").feature("aDef").set("cachepattern", true);
    model.sol("sol1").feature("s1").feature("aDef").set("matherr", true);
    model.sol("sol1").feature("s1").feature("aDef").set("blocksizeactive", false);
    model.sol("sol1").feature("s1").feature("aDef").set("nullfun", "auto");
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.param().set("freq0", "2000[Hz]");

    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.component("comp1").geom("geom1").feature("r1").set("size", new int[]{1, 2});

    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.component("comp1").physics("acpr").create("pwr1", "PlaneWaveRadiation", 1);
    model.component("comp1").physics("acpr").feature("pwr1").selection().set(2, 3);

    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.component("comp1").physics("acpr").feature().remove("port1");
    model.component("comp1").physics("acpr").feature().remove("port2");
    model.component("comp1").physics("acpr").create("pwr2", "PlaneWaveRadiation", 1);
    model.component("comp1").physics("acpr").feature("pwr2").selection().set(1);
    model.component("comp1").physics("acpr").feature().remove("pwr2");
    model.component("comp1").physics("acpr").create("imp1", "Impedance", 1);
    model.component("comp1").physics("acpr").feature("imp1").selection().set(1);
    model.component("comp1").physics("acpr").feature().remove("imp1");
    model.component("comp1").physics("acpr").create("mb1", "Matched", 1);
    model.component("comp1").physics("acpr").feature().remove("mb1");
    model.component("comp1").physics("acpr").create("port1", "Port", 1);
    model.component("comp1").physics("acpr").feature("port1").selection().set(1);
    model.component("comp1").physics("acpr").feature("port1").set("pamp", "exp(-y^2/2/(0.3[m])^2)");
    model.component("comp1").physics("acpr").feature("pwr1").selection().set(2, 3, 4);
    model.component("comp1").physics("acpr").feature("pwr1").create("ipf1", "IncidentPressureField", 1);
    model.component("comp1").physics("acpr").feature("pwr1").selection().set(1);
    model.component("comp1").physics("acpr").feature("port1").selection().set();
    model.component("comp1").physics("acpr").feature("pwr1").feature("ipf1").set("c", "c0");
    model.component("comp1").physics("acpr").feature("pwr1").feature("ipf1").set("pamp", "exp(-y^2/2/(0.3[m])^2)");
    model.component("comp1").physics("acpr").feature().remove("port1");
    model.component("comp1").physics("acpr").create("pwr2", "PlaneWaveRadiation", 1);
    model.component("comp1").physics("acpr").feature("pwr2").selection().set(2, 3, 4);

    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.component("comp1").material("mat2").selection().set();
    model.component("comp1").material("mat1").selection().set(1, 2);

    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.component("comp1").geom("geom1").feature("r1").set("size", new double[]{1.5, 2});

    return model;
  }

  public static Model run2(Model model) {

    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.component("comp1").physics("acpr").feature("pwr1").feature("ipf1").set("pamp", "exp(-y^2/2/(0.2[m])^2)");

    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.param().set("freq0", "3000[Hz]");

    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.component("comp1").material("mat2").selection().set(2);

    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.component("comp1").geom("geom1").feature("r1").set("size", new String[]{"1.0", "1.5"});

    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.component("comp1").material("mat2").selection().set();

    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.component("comp1").material("mat2").selection().set(2);

    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.label("acoustic2d_cylcscatter.mph");

    model.result("pg1").run();

    return model;
  }

  public static void main(String[] args) {
    Model model = run();
    run2(model);
  }

}
