/*
 * SRR_tr_unit1_v62.java
 */

import com.comsol.model.*;
import com.comsol.model.util.*;

/** Model exported on Oct 11 2026, 02:46 by COMSOL 6.2.0.290. */
public class SRR_tr_unit1_v62 {

  public static Model run() {
    Model model = ModelUtil.create("Model");

    model.modelPath("C:\\Users\\jense\\Documents\\codes\\FullMeta");

    model.modelNode().create("comp1");

    model.geom().create("geom1", 3);

    model.mesh().create("mesh1", "geom1");

    model.physics().create("emw", "ElectromagneticWaves", "geom1");

    model.param().set("r", "sqrt(x^2+y^2+z^2)");
    model.param().set("rho", "sqrt(x^2+y^2)");
    model.param().set("theta", "acos(z/r)");
    model.param().set("phi", "arg(x+j*y)");
    model.param().set("Exloc", "-j*omega0*(Byloc*z-Bzloc*y)/2");
    model.param().set("Eyloc", "-j*omega0*(Bzloc*x-Bxloc*z)/2");
    model.param().set("Ezloc", "-j*omega0*(Bxloc*y-Byloc*x)/2");
    model.param().set("Bxloc", "0.3[A/m]*mu0");
    model.param().set("Byloc", "0.7[A/m]*mu0");
    model.param().set("Bzloc", "0.5[A/m]*mu0");
    model.param().remove("r");
    model.param().remove("rho");
    model.param().remove("theta");
    model.param().remove("phi");
    model.param().remove("Exloc");
    model.param().remove("Eyloc");
    model.param().remove("Ezloc");
    model.param().remove("Bxloc");
    model.param().remove("Byloc");
    model.param().remove("Bzloc");

    model.variable().create("var1");
    model.variable("var1").model("comp1");

    model.geom("geom1").run();

    model.variable().remove("var1");
    model.variable().create("var1");
    model.variable("var1").set("r", "sqrt(x^2+y^2+z^2)");
    model.variable("var1").set("rho", "sqrt(x^2+y^2)");
    model.variable("var1").set("theta", "acos(z/r)");
    model.variable("var1").set("phi", "arg(x+j*y)");
    model.variable("var1").set("Exloc", "-j*omega0*(Byloc*z-Bzloc*y)/2");
    model.variable("var1").set("Eyloc", "-j*omega0*(Bzloc*x-Bxloc*z)/2");
    model.variable("var1").set("Ezloc", "-j*omega0*(Bxloc*y-Byloc*x)/2");
    model.variable("var1").set("Bxloc", "0.3[A/m]*mu0");
    model.variable("var1").set("Byloc", "0.7[A/m]*mu0");
    model.variable("var1").set("Bzloc", "0.5[A/m]*mu0");

    model.param().set("eps0", "epsilon0_const");
    model.param().set("mu0", "mu0_const");
    model.param().set("c", "1/sqrt(eps0*mu0)");
    model.param().set("freq", "1[GHz]");
    model.param().set("omega0", "2*pi*freq");
    model.param().set("k0", "2");
    model.param().rename("k0", "lambda0");
    model.param().set("lambda0", "c/freq");
    model.param().set("k0", "2*pi/lambda0");
    model.param().set("a1", "5e-4");
    model.param().set("a2", "a1");
    model.param().set("a3", "a1");

    model.geom("geom1").create("blk1", "Block");
    model.geom("geom1").feature("blk1").set("size", new String[]{"a1", "a2", "a3"});
    model.geom("geom1").feature("blk1").set("base", "center");
    model.geom("geom1").run("blk1");
    model.geom("geom1").create("cyl1", "Cylinder");
    model.geom("geom1").feature("cyl1").set("r", "0.2e-4");
    model.geom("geom1").feature("cyl1").set("h", "a3");
    model.geom("geom1").feature("cyl1").set("pos", new String[]{"0", "0", "-a3/2"});
    model.geom("geom1").run("fin");
    model.geom("geom1").feature("cyl1").set("r", "2e-4");
    model.geom("geom1").run("fin");

    model.param().set("V", "a1*a2*a3");
    model.param().set("Rcyl", "1e-4");
    model.param().set("Acyl", "pi*Rcyl^2");

    model.geom("geom1").feature("cyl1").set("r", "Rcyl");
    model.geom("geom1").run("fin");

    model.mesh().create("mesh2", "geom1");
    model.mesh().remove("mesh2");
    model.mesh("mesh1").create("edg1", "Edge");
    model.mesh("mesh1").feature("edg1").selection().set(new int[]{1, 2, 3});
    model.mesh("mesh1").feature("edg1").create("size1", "Size");
    model.mesh("mesh1").feature("edg1").feature("size1").set("custom", "on");
    model.mesh("mesh1").feature("edg1").feature("size1").set("hmaxactive", "on");
    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").feature("edg1").feature("size1").set("hmax", "1.0E-4");
    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").feature("edg1").feature("size1").set("hmax", "1.5E-4");
    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").feature("edg1").feature("size1").set("hmax", "1.2E-4");
    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").feature("edg1").feature("size1").set("hmax", "1.5E-4");
    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").feature("edg1").feature("size1").set("hmax", "1.4E-4");
    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").feature("edg1").feature("size1").set("hmax", "1.3E-4");
    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").feature("edg1").feature("size1").set("hmax", "1.5E-4");
    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").create("cpe1", "CopyEdge");
    model.mesh("mesh1").feature("cpe1").selection("source").set(new int[]{1});
    model.mesh("mesh1").feature("cpe1").selection("destination").set(new int[]{6, 21, 24});
    model.mesh("mesh1").run("cpe1");
    model.mesh("mesh1").create("cpe2", "CopyEdge");
    model.mesh("mesh1").feature("cpe2").selection("source").set(new int[]{3});
    model.mesh("mesh1").feature("cpe2").selection("destination").set(new int[]{5, 7, 8});
    model.mesh("mesh1").run("cpe2");
    model.mesh("mesh1").create("cpe3", "CopyEdge");
    model.mesh("mesh1").feature("cpe3").selection("source").set(new int[]{2});
    model.mesh("mesh1").feature("cpe3").selection("destination").set(new int[]{4, 22, 23});
    model.mesh("mesh1").run("cpe3");
    model.mesh("mesh1").feature("cpe1").selection("destination").remove(new int[]{24});
    model.mesh("mesh1").run("cpe1");
    model.mesh("mesh1").run("cpe1");
    model.mesh("mesh1").feature("cpe2").selection("destination").remove(new int[]{8});
    model.mesh("mesh1").run("cpe2");
    model.mesh("mesh1").feature("cpe3").selection("destination").remove(new int[]{23});
    model.mesh("mesh1").run("cpe3");
    model.mesh("mesh1").create("ftri1", "FreeTri");
    model.mesh("mesh1").feature("ftri1").selection().set(new int[]{1, 2, 3});
    model.mesh("mesh1").run("ftri1");
    model.mesh("mesh1").create("cpf1", "CopyFace");
    model.mesh("mesh1").feature("cpf1").selection("source").set(new int[]{1});
    model.mesh("mesh1").feature("cpf1").selection("destination").set(new int[]{12});
    model.mesh("mesh1").run("cpf1");
    model.mesh("mesh1").create("cpf2", "CopyFace");
    model.mesh("mesh1").feature("cpf2").selection("source").set(new int[]{2});
    model.mesh("mesh1").feature("cpf2").selection("destination").set(new int[]{5});
    model.mesh("mesh1").run("cpf2");
    model.mesh("mesh1").create("cpf3", "CopyFace");
    model.mesh("mesh1").feature("cpf3").selection("source").set(new int[]{3});
    model.mesh("mesh1").feature("cpf3").selection("destination").set(new int[]{4});
    model.mesh("mesh1").run("cpf3");
    model.mesh("mesh1").feature("cpf3").selection("destination").set(new int[]{8});
    model.mesh("mesh1").feature("cpf3").selection("source").set(new int[]{3, 8});
    model.mesh("mesh1").feature("cpf3").selection("destination").set(new int[]{4, 9});
    model.mesh("mesh1").feature("ftri1").selection().set(new int[]{1, 2, 3, 8});
    model.mesh("mesh1").run("ftri1");
    model.mesh("mesh1").run("cpf1");
    model.mesh("mesh1").run("cpf2");
    model.mesh("mesh1").run("cpf3");
    model.mesh("mesh1").create("ftet1", "FreeTet");
    model.mesh("mesh1").feature("ftet1").create("size1", "Size");
    model.mesh("mesh1").feature("size").set("custom", "on");
    model.mesh("mesh1").feature("edg1").feature().remove("size1");
    model.mesh("mesh1").feature("ftet1").feature().remove("size1");
    model.mesh("mesh1").feature("size").set("hmax", "1.5E-5");
    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").feature("edg1").create("size1", "Size");
    model.mesh("mesh1").feature("size").set("hmax", "meshsize");

    model.param().set("meshsize", "1.5e-5");

    model.mesh("mesh1").feature("edg1").feature("size1").set("custom", "on");
    model.mesh("mesh1").feature("edg1").feature("size1").set("hmaxactive", "on");
    model.mesh("mesh1").feature("edg1").feature("size1").set("hmax", "meshsize");
    model.mesh("mesh1").feature("ftri1").create("size1", "Size");
    model.mesh("mesh1").feature("ftri1").feature("size1").set("custom", "on");
    model.mesh("mesh1").feature("ftri1").feature("size1").set("hmaxactive", "on");
    model.mesh("mesh1").feature("ftri1").feature("size1").set("hmax", "meshsize");
    model.mesh("mesh1").feature("ftet1").create("size1", "Size");
    model.mesh("mesh1").feature("ftet1").feature("size1").set("custom", "on");
    model.mesh("mesh1").feature("ftet1").feature("size1").set("hmaxactive", "on");
    model.mesh("mesh1").feature("ftet1").feature("size1").set("hmax", "meshsize");
    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").run();

    model.param().set("meshsize", "1.5e-4");

    model.mesh("mesh1").run("edg1");

    model.param().set("meshsize", "1.3e-4");

    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").run("edg1");

    model.param().set("meshsize", "1.0e-4");

    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").run();

    model.material().create("mat1", "Common", "comp1");
    model.material("mat1").propertyGroup("def").set("relpermittivity", new String[]{"1"});
    model.material("mat1").propertyGroup("def").set("relpermeability", new String[]{"1"});
    model.material("mat1").propertyGroup("def").set("electricconductivity", new String[]{"0"});
    model.material().create("mat2", "Common", "comp1");
    model.material("mat1").selection().set(new int[]{1});
    model.material("mat2").selection().set(new int[]{2});
    model.material("mat2").propertyGroup("def").set("relpermittivity", new String[]{"1"});
    model.material("mat2").propertyGroup("def").set("relpermeability", new String[]{"1"});
    model.material("mat2").propertyGroup("def").set("electricconductivity", new String[]{"0"});

    model.physics("emw").prop("BackgroundField").set("SolveFor", "scatteredField");
    model.physics("emw").prop("BackgroundField").set("Eb", new String[]{"Exloc", "Eyloc", "Ezloc"});
    model.physics("emw").feature().create("pc1", "PeriodicCondition", 2);
    model.physics("emw").feature("pc1").selection().set(new int[]{1, 12});
    model.physics("emw").feature().create("pc2", "PeriodicCondition", 2);
    model.physics("emw").feature("pc2").selection().set(new int[]{2, 5});
    model.physics("emw").feature().create("pc3", "PeriodicCondition", 2);
    model.physics("emw").feature("pc3").selection().set(new int[]{3, 4, 8, 9});

    model.study().create("std1");
    model.study("std1").create("freq", "Frequency");
    model.study("std1").feature("freq").activate("emw", true);
    model.study("std1").feature("freq").set("plist", "freq");

    model.sol().create("sol1");
    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "Default");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hx");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hy");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hz");
    model.result("pg1").run();

    model.label("template_3dHloc_v50.mph");

    model.result("pg1").run();
    model.result("pg1").run();

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.param().set("omega0", "2*pi*freq0");
    model.param().set("lambda0", "c/freq0");
    model.param().rename("freq", "freq0");
    model.param().set("freq0", "1[GHz]");

    model.study("std1").feature("freq").set("plist", "freq0");

    model.result().dataset().remove("dset1");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "Default");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.result().dataset().create("dset1", "Solution");
    model.result().dataset("dset1").set("solution", "sol1");
    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("solvertype", "none");
    model.result("pg1").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("solvertype", "none");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hz");
    model.result("pg1").run();

    model.study("std1").create("param", "Parametric");
    model.study("std1").feature("param").setIndex("pname", "eps0", 0);
    model.study("std1").feature("param").setIndex("plistarr", "", 0);
    model.study("std1").feature("param").setIndex("punit", "", 0);
    model.study("std1").feature("param").setIndex("pname", "eps0", 0);
    model.study("std1").feature("param").setIndex("plistarr", "", 0);
    model.study("std1").feature("param").setIndex("punit", "", 0);
    model.study("std1").feature("param").setIndex("pname", "freq0", 0);
    model.study("std1").feature("param").setIndex("plistarr", "range(2[GHz],1[GHz],3[GHz])", 0);

    model.result().dataset().remove("dset1");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "Default");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch().create("p1", "Parametric");
    model.batch("p1").study("std1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "none");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(2[GHz],1[GHz],3[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").set("pdistrib", "off");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");

    model.sol().create("sol2");
    model.sol("sol2").study("std1");
    model.sol("sol2").label("Parametric Solutions 1");

    model.batch("p1").feature("so1").set("psol", "sol2");

    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "1"});
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hz");
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "2"});
    model.result("pg1").run();

    model.study("std1").feature("param").setIndex("plistarr", "range(1[GHz],5[GHz],11[GHz])", 0);

    model.result().dataset().remove("dset1");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "Default");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(1[GHz],5[GHz],11[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").set("pdistrib", "off");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");

    model.result().dataset().create("dset1", "Solution");
    model.result().dataset("dset1").set("solution", "sol2");
    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("outersolnum", 1);
    model.result("pg1").set("solvertype", "none");
    model.result("pg1").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("outertype", "none");
    model.result("pg1").feature("mslc1").set("solvertype", "none");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hz");
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "2"});
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "3"});
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "2"});
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "1"});
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hx");
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "2"});
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "1"});
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "2"});
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "3"});
    model.result("pg1").run();

    model.physics("emw").feature().create("sctr1", "Scattering", 2);
    model.physics("emw").feature("sctr1").selection().set(new int[]{1, 2, 3, 4, 5, 8, 9, 12});
    model.physics("emw").feature("pc1").active(false);
    model.physics("emw").feature("pc2").active(false);
    model.physics("emw").feature("pc3").active(false);

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");

    return model;
  }

  public static Model run2(Model model) {
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "bicgstab");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("sv1")
         .set("prefun", "sorvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "i1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(1[GHz],5[GHz],11[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").set("pdistrib", "off");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");

    model.result().dataset().create("dset2", "Solution");
    model.result().dataset("dset2").set("solution", "sol1");
    model.result().create("pg2", "PlotGroup3D");
    model.result("pg2").label("Electric Field (emw) 1");
    model.result("pg2").set("data", "dset2");
    model.result("pg2").set("oldanalysistype", "noneavailable");
    model.result("pg2").set("solvertype", "none");
    model.result("pg2").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg2").set("frametype", "spatial");
    model.result("pg2").set("data", "dset2");
    model.result("pg2").feature().create("mslc1", "Multislice");
    model.result("pg2").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg2").feature("mslc1").set("solvertype", "none");
    model.result("pg2").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "1"});
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "2"});
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").run();

    model.study("std1").feature("param").setIndex("plistarr", "range(11[GHz],5[GHz],21[GHz])", 0);

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "bicgstab");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("sv1")
         .set("prefun", "sorvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "i1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(11[GHz],5[GHz],21[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").set("pdistrib", "off");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");
    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "1"});
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "2"});
    model.result("pg1").run();
    model.result("pg1").run();

    model.study("std1").feature("param").setIndex("plistarr", "range(101[GHz],5[GHz],105[GHz])", 0);
    model.study("std1").feature("param").setIndex("plistarr", "range(101[GHz],5[GHz],106[GHz])", 0);

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "bicgstab");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("sv1")
         .set("prefun", "sorvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "i1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(101[GHz],5[GHz],106[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").set("pdistrib", "off");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");
    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "1"});
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "2"});
    model.result("pg1").run();

    model.label("template_3dHloc_v50.mph");

    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "1"});
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "2"});
    model.result("pg1").run();
    model.result("pg1").run();
    model.result().dataset().remove("dset1");
    model.result().dataset().remove("dset2");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "Default");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "bicgstab");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("sv1")
         .set("prefun", "sorvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "i1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(101[GHz],5[GHz],106[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").set("pdistrib", "off");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");

    model.result().dataset().create("dset1", "Solution");
    model.result().dataset("dset1").set("solution", "sol2");
    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("outersolnum", 1);
    model.result("pg1").set("solvertype", "none");
    model.result("pg1").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("outertype", "none");
    model.result("pg1").feature("mslc1").set("solvertype", "none");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hz");
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "2"});
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "1"});
    model.result("pg1").run();

    model.study("std1").feature("param").setIndex("plistarr", "range(1[GHz],100[GHz],101[GHz])", 0);

    model.result().dataset().remove("dset1");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "Default");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "bicgstab");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("sv1")
         .set("prefun", "sorvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "i1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(1[GHz],100[GHz],101[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").set("pdistrib", "off");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");

    model.result().dataset().create("dset1", "Solution");
    model.result().dataset("dset1").set("solution", "sol2");
    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("outersolnum", 1);
    model.result("pg1").set("solvertype", "none");
    model.result("pg1").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("outertype", "none");
    model.result("pg1").feature("mslc1").set("solvertype", "none");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hz");
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "2"});
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "1"});
    model.result("pg1").run();

    model.physics("emw").feature("sctr1").active(false);
    model.physics("emw").feature("pc1").active(true);
    model.physics("emw").feature("pc2").active(true);
    model.physics("emw").feature("pc3").active(true);

    model.result().dataset().remove("dset1");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "Default");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(1[GHz],100[GHz],101[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").set("pdistrib", "off");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");

    model.result().dataset().create("dset1", "Solution");
    model.result().dataset("dset1").set("solution", "sol2");
    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("outersolnum", 1);
    model.result("pg1").set("solvertype", "none");
    model.result("pg1").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("outertype", "none");
    model.result("pg1").feature("mslc1").set("solvertype", "none");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hz");
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "2"});
    model.result("pg1").run();
    model.result("pg1").set("looplevel", new String[]{"1", "1"});
    model.result("pg1").run();

    model.physics("emw").feature().remove("sctr1");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(1[GHz],100[GHz],101[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").set("pdistrib", "off");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");

    model.result().dataset().create("dset2", "Solution");
    model.result().dataset("dset2").set("solution", "sol1");
    model.result().create("pg2", "PlotGroup3D");
    model.result("pg2").label("Electric Field (emw) 1");
    model.result("pg2").set("data", "dset2");
    model.result("pg2").set("oldanalysistype", "noneavailable");
    model.result("pg2").set("solvertype", "none");
    model.result("pg2").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg2").set("frametype", "spatial");
    model.result("pg2").set("data", "dset2");
    model.result("pg2").feature().create("mslc1", "Multislice");
    model.result("pg2").feature("mslc1").set("oldanalysistype", "noneavailable");

    return model;
  }

  public static Model run3(Model model) {
    model.result("pg2").feature("mslc1").set("solvertype", "none");
    model.result("pg2").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg2").run();
    model.result().dataset().remove("dset1");
    model.result().dataset().remove("dset2");

    model.study("std1").feature("param").active(false);

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "Default");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch().remove("p1");

    model.result().dataset().create("dset1", "Solution");
    model.result().dataset("dset1").set("solution", "sol1");
    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("solvertype", "none");
    model.result("pg1").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("solvertype", "none");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hz");
    model.result("pg1").run();

    model.cpl().create("intop1", "Integration", "geom1");
    model.cpl("intop1").set("opname", "intdS");
    model.cpl("intop1").selection().geom("geom1", 2);
    model.cpl("intop1").selection().set(new int[]{1, 2, 3, 4, 5, 8, 9, 12});

    model.variable().create("var2");
    model.variable("var2").set("Exeff", "mod1.intdS(-(Ey*nx*y)+Ex*ny*y-Ez*nx*z+Ex*nz*z)/2/V/eta0");
    model.variable("var2").set("Eyeff", "mod1.intdS(Ey*nx*x-Ex*ny*x-Ez*ny*z+Ey*nz*z)/2/V/eta0");
    model.variable("var2").set("Ezeff", "mod1.intdS(Ez*nx*x-Ex*nz*x+Ez*ny*y-Ey*nz*y)/2/V/eta0");
    model.variable("var2").set("Hxeff", "mod1.intdS(-(emw2.Hy*nx*y)+emw2.Hx*ny*y-emw2.Hz*nx*z+emw2.Hx*nz*z)/2/V");
    model.variable("var2").set("Hyeff", "mod1.intdS(emw2.Hy*nx*x-emw2.Hx*ny*x-emw2.Hz*ny*z+emw2.Hy*nz*z)/2/V");
    model.variable("var2").set("Hzeff", "mod1.intdS(emw2.Hz*nx*x-emw2.Hx*nz*x+emw2.Hz*ny*y-emw2.Hy*nz*y)/2/V");
    model.variable("var2").set("Dxeff", "mod1.intdS(-(emw.Hz*ny)+emw.Hy*nz)/(-j*k0*V)");
    model.variable("var2").set("Dyeff", "mod1.intdS(emw.Hz*nx-emw.Hx*nz)/(-j*k0*V)");
    model.variable("var2").set("Dzeff", "mod1.intdS(-(emw.Hy*nx)+emw.Hx*ny)/(-j*k0*V)");
    model.variable("var2").set("Bxeff", "mod1.intdS(-(E2z*ny)+E2y*nz)/(j*k0*V)/eta0");
    model.variable("var2").set("Byeff", "mod1.intdS(E2z*nx-E2x*nz)/(j*k0*V)/eta0");
    model.variable("var2").set("Bzeff", "mod1.intdS(-(E2y*nx)+E2x*ny)/(j*k0*V)/eta0");
    model.variable("var2").set("muxeff", "Bxeff/Hxeff");
    model.variable("var2").set("muyeff", "Byeff/Hyeff");
    model.variable("var2").set("muzeff", "Bzeff/Hzeff");
    model.variable("var2").set("epsxeff", "Dxeff/Exeff");
    model.variable("var2").set("epsyeff", "Dyeff/Eyeff");
    model.variable("var2").set("epszeff", "Dzeff/Ezeff");
    model.variable("var2").set("Dxeff", "mod1.intdS(-emw.Hz*ny+emw.Hy*nz)/(-j*k0*V)");

    model.param().set("a1", "5e-4[m]");
    model.param().set("Rcyl", "1e-4[m]");
    model.param().set("meshsize", "1.0e-4[m]");

    model.variable("var2").set("Dxeff", "comp1.intdS(-emw.Hz*ny+emw.Hy*nz)/(-j*k0*V)");
    model.variable("var2").set("Dyeff", "comp1.intdS(emw.Hz*nx-emw.Hx*nz)/(-j*k0*V)");
    model.variable("var2").set("Dzeff", "comp1.intdS(-(emw.Hy*nx)+emw.Hx*ny)/(-j*k0*V)");
    model.variable("var2").set("Bxeff", "comp1.intdS(-(E2z*ny)+E2y*nz)/(j*k0*V)/eta0");
    model.variable("var2").set("Byeff", "comp1.intdS(E2z*nx-E2x*nz)/(j*k0*V)/eta0");
    model.variable("var2").set("Bzeff", "comp1.intdS(-(E2y*nx)+E2x*ny)/(j*k0*V)/eta0");
    model.variable("var2").set("Exeff", "comp1.intdS(-(Ey*nx*y)+Ex*ny*y-Ez*nx*z+Ex*nz*z)/2/V/eta0");
    model.variable("var2").set("Eyeff", "comp1.intdS(Ey*nx*x-Ex*ny*x-Ez*ny*z+Ey*nz*z)/2/V/eta0");
    model.variable("var2").set("Ezeff", "comp1.intdS(Ez*nx*x-Ex*nz*x+Ez*ny*y-Ey*nz*y)/2/V/eta0");
    model.variable("var2").set("Hxeff", "comp1.intdS(-(emw2.Hy*nx*y)+emw2.Hx*ny*y-emw2.Hz*nx*z+emw2.Hx*nz*z)/2/V");
    model.variable("var2").set("Hyeff", "comp1.intdS(emw2.Hy*nx*x-emw2.Hx*ny*x-emw2.Hz*ny*z+emw2.Hy*nz*z)/2/V");
    model.variable("var2").set("Hzeff", "comp1.intdS(emw2.Hz*nx*x-emw2.Hx*nz*x+emw2.Hz*ny*y-emw2.Hy*nz*y)/2/V");
    model.variable("var2").set("Hxeff", "comp1.intdS(-emw2.Hy*nx*y+emw2.Hx*ny*y-emw2.Hz*nx*z+emw2.Hx*nz*z)/2/V");
    model.variable("var2").set("Exeff", "comp1.intdS(-Ey*nx*y+Ex*ny*y-Ez*nx*z+Ex*nz*z)/2/V/eta0");
    model.variable("var2").set("Dzeff", "comp1.intdS(-emw.Hy*nx+emw.Hx*ny)/(-j*k0*V)");
    model.variable("var2").set("Bxeff", "comp1.intdS(-E2z*ny+E2y*nz)/(j*k0*V)/eta0");
    model.variable("var2").set("Bzeff", "comp1.intdS(-E2y*nx+E2x*ny)/(j*k0*V)/eta0");
    model.variable("var2").set("Exeff", "comp1.intdS(-emw.Ey*nx*y+emw.Ex*ny*y-emw.Ez*nx*z+emw.Ex*nz*z)/2/V/eta0");

    model.param().set("eta0", "sqrt(mu0/eps0)");

    model.variable("var2").set("Eyeff", "comp1.intdS(emw.Ey*nx*x-emw.Ex*ny*x-emw.Ez*ny*z+emw.Ey*nz*z)/2/V/eta0");
    model.variable("var2").set("Ezeff", "comp1.intdS(emw.Ez*nx*x-emw.Ex*nz*x+emw.Ez*ny*y-emw.Ey*nz*y)/2/V/eta0");
    model.variable("var2").set("Hxeff", "comp1.intdS(-emw.Hy*nx*y+emw.Hx*ny*y-emw.Hz*nx*z+emw.Hx*nz*z)/2/V");
    model.variable("var2").set("Hyeff", "comp1.intdS(emw.Hy*nx*x-emw.Hx*ny*x-emw.Hz*ny*z+emw.Hy*nz*z)/2/V");
    model.variable("var2").set("Hzeff", "comp1.intdS(emw.Hz*nx*x-emw.Hx*nz*x+emw.Hz*ny*y-emw.Hy*nz*y)/2/V");
    model.variable("var2").set("Bxeff", "comp1.intdS(-emw.Ez*ny+emw.Ey*nz)/(j*k0*V)/eta0");
    model.variable("var2").set("Byeff", "comp1.intdS(emw.Ez*nx-emw.Ex*nz)/(j*k0*V)/eta0");
    model.variable("var2").set("Bzeff", "comp1.intdS(-emw.Ey*nx+emw.Ex*ny)/(j*k0*V)/eta0");

    model.result("pg1").run();

    model.variable("var2").set("aa", "comp1.intdS(emw.Ez)");

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Ez");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hz");
    model.result("pg1").run();

    model.variable("var2").set("aa", "comp1.intdS(emw.Hz)");
    model.variable().create("var3");
    model.variable("var3").model("comp1");
    model.variable("var2").remove("aa");
    model.variable("var3").set("Exeff", "comp1.intdS(-emw.Ey*nx*y+emw.Ex*ny*y-emw.Ez*nx*z+emw.Ex*nz*z)/2/V/eta0");
    model.variable("var3").set("Eyeff", "comp1.intdS(emw.Ey*nx*x-emw.Ex*ny*x-emw.Ez*ny*z+emw.Ey*nz*z)/2/V/eta0");
    model.variable("var3").set("Ezeff", "comp1.intdS(emw.Ez*nx*x-emw.Ex*nz*x+emw.Ez*ny*y-emw.Ey*nz*y)/2/V/eta0");
    model.variable("var3").set("Hxeff", "comp1.intdS(-emw.Hy*nx*y+emw.Hx*ny*y-emw.Hz*nx*z+emw.Hx*nz*z)/2/V");
    model.variable("var3").set("Hyeff", "comp1.intdS(emw.Hy*nx*x-emw.Hx*ny*x-emw.Hz*ny*z+emw.Hy*nz*z)/2/V");
    model.variable("var3").set("Hzeff", "comp1.intdS(emw.Hz*nx*x-emw.Hx*nz*x+emw.Hz*ny*y-emw.Hy*nz*y)/2/V");
    model.variable("var3").set("Dxeff", "comp1.intdS(-emw.Hz*ny+emw.Hy*nz)/(-j*k0*V)");
    model.variable("var3").set("Dyeff", "comp1.intdS(emw.Hz*nx-emw.Hx*nz)/(-j*k0*V)");
    model.variable("var3").set("Dzeff", "comp1.intdS(-emw.Hy*nx+emw.Hx*ny)/(-j*k0*V)");
    model.variable("var3").set("Bxeff", "comp1.intdS(-emw.Ez*ny+emw.Ey*nz)/(j*k0*V)/eta0");
    model.variable("var3").set("Byeff", "comp1.intdS(emw.Ez*nx-emw.Ex*nz)/(j*k0*V)/eta0");
    model.variable("var3").set("Bzeff", "comp1.intdS(-emw.Ey*nx+emw.Ex*ny)/(j*k0*V)/eta0");
    model.variable("var3").set("muxeff", "Bxeff/Hxeff");
    model.variable("var3").set("muyeff", "Byeff/Hyeff");
    model.variable("var3").set("muzeff", "Bzeff/Hzeff");
    model.variable("var3").set("epsxeff", "Dxeff/Exeff");
    model.variable("var3").set("epsyeff", "Dyeff/Eyeff");
    model.variable("var3").set("epszeff", "Dzeff/Ezeff");
    model.variable().remove("var2");
    model.variable("var3").set("Exeff", "intdS(-emw.Ey*nx*y+emw.Ex*ny*y-emw.Ez*nx*z+emw.Ex*nz*z)/2/V/eta0");
    model.variable("var3").set("Eyeff", "intdS(emw.Ey*nx*x-emw.Ex*ny*x-emw.Ez*ny*z+emw.Ey*nz*z)/2/V/eta0");
    model.variable("var3").set("Ezeff", "intdS(emw.Ez*nx*x-emw.Ex*nz*x+emw.Ez*ny*y-emw.Ey*nz*y)/2/V/eta0");
    model.variable("var3").set("Hxeff", "intdS(-emw.Hy*nx*y+emw.Hx*ny*y-emw.Hz*nx*z+emw.Hx*nz*z)/2/V");
    model.variable("var3").set("Hyeff", "intdS(emw.Hy*nx*x-emw.Hx*ny*x-emw.Hz*ny*z+emw.Hy*nz*z)/2/V");
    model.variable("var3").set("Hzeff", "intdS(emw.Hz*nx*x-emw.Hx*nz*x+emw.Hz*ny*y-emw.Hy*nz*y)/2/V");
    model.variable("var3").set("Dxeff", "intdS(-emw.Hz*ny+emw.Hy*nz)/(-j*k0*V)");
    model.variable("var3").set("Dyeff", "intdS(emw.Hz*nx-emw.Hx*nz)/(-j*k0*V)");
    model.variable("var3").set("Dzeff", "intdS(-emw.Hy*nx+emw.Hx*ny)/(-j*k0*V)");
    model.variable("var3").set("Bxeff", "intdS(-emw.Ez*ny+emw.Ey*nz)/(j*k0*V)/eta0");
    model.variable("var3").set("Byeff", "intdS(emw.Ez*nx-emw.Ex*nz)/(j*k0*V)/eta0");
    model.variable("var3").set("Bzeff", "intdS(-emw.Ey*nx+emw.Ex*ny)/(j*k0*V)/eta0");
    model.variable("var3").set("Dxeff", "intdS(-emw.Hz*ny+emw.Hy*nz)/(-j*omega0*V)");
    model.variable("var1").set("Dxloc", "0.1[V/m]*eps0");
    model.variable("var1").remove("Dxloc");
    model.variable("var3").set("Dyeff", "intdS(emw.Hz*nx-emw.Hx*nz)/(-j*omega0*V)");
    model.variable("var3").set("Dzeff", "intdS(-emw.Hy*nx+emw.Hx*ny)/(-j*omega0*V)");
    model.variable("var3").set("Bxeff", "intdS(-emw.Ez*ny+emw.Ey*nz)/(j*omega0*V)");
    model.variable("var3").set("Byeff", "intdS(emw.Ez*nx-emw.Ex*nz)/(j*omega0*V)");
    model.variable("var3").set("Bzeff", "intdS(-emw.Ey*nx+emw.Ex*ny)/(j*omega0*V)");
    model.variable("var3").set("Exeff", "intdS(-emw.Ey*nx*y+emw.Ex*ny*y-emw.Ez*nx*z+emw.Ex*nz*z)/2/V");
    model.variable("var3").set("Eyeff", "intdS(emw.Ey*nx*x-emw.Ex*ny*x-emw.Ez*ny*z+emw.Ey*nz*z)/2/V");
    model.variable("var3").set("Ezeff", "intdS(emw.Ez*nx*x-emw.Ex*nz*x+emw.Ez*ny*y-emw.Ey*nz*y)/2/V");
    model.variable("var3").set("epsxeff", "Dxeff/Exeff/eps0");
    model.variable("var3").set("epsyeff", "Dyeff/Eyeff/eps0");
    model.variable("var3").set("epszeff", "Dzeff/Ezeff/eps0");
    model.variable("var3").set("muxeff", "Bxeff/Hxeff/mu0");
    model.variable("var3").set("muyeff", "Byeff/Hyeff/mu0");
    model.variable("var3").set("muzeff", "Bzeff/Hzeff/mu0");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result().numerical().create("gev1", "EvalGlobal");
    model.result().numerical("gev1").set("expr", "Bxeff");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (Bxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "Bxeff/mu0");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().table().remove("tbl1");
    model.result().numerical("gev1").set("expr", "Byeff/mu0");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (Byeff/mu0)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "Bzeff/mu0");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result("pg1").run();
    model.result("pg1").run();
    model.result().numerical("gev1").set("expr", "Exeff/eta0");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "Eyeff/eta0");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "Ezeff/eta0");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().table().remove("tbl1");
    model.result().numerical("gev1").set("expr", "Hxeff");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (Hxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "Hyeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "Hzeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().table().remove("tbl1");
    model.result().numerical("gev1").set("expr", "Dxeff/eps0");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (Dxeff/eps0)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "Dxeff/c");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "Dxeff*c");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().table().remove("tbl1");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (Dxeff*c)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "Dyeff*c");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "Dzeff*c");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "Hxeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().table().remove("tbl1");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (Hxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();

    model.material("mat2").propertyGroup("def").set("relpermeability", new String[]{"2"});

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hx");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hy");
    model.result("pg1").run();
    model.result().table().remove("tbl1");
    model.result().numerical("gev1").set("expr", "muxeff");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "muzeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();

    model.material("mat2").propertyGroup("def").set("relpermeability", new String[]{"-100.0"});

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result().table().remove("tbl1");
    model.result().numerical("gev1").set("expr", "muxeff");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "muzeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();

    model.label("template_Hloc3d_v50.mph");
    model.label("template_Hloc3d_v50.mph");

    model.mesh("mesh1").clearMesh();

    model.result().dataset().remove("dset1");

    model.sol("sol1").clearSolution();
    model.sol("sol2").clearSolution();
    model.sol("sol3").clearSolution();
    model.sol("sol4").clearSolution();
    model.sol().remove("sol2");
    model.sol().remove("sol1");

    model.label("template_Hloc3d_v50.mph");

    model.sol().create("sol1");
    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "Default");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hz");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "emw.Hx");
    model.result("pg1").run();
    model.result().numerical().create("gev1", "EvalGlobal");
    model.result().numerical("gev1").set("expr", "muxeff");
    model.result().table().remove("tbl1");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "muzeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();

    model.label("template_Hloc3d_v50.mph");

    model.variable().create("var4");
    model.variable("var4").model("comp1");
    model.variable("var4").label("Variables 2");
    model.variable().move("var4", 0);
    model.variable("var4").set("Ex", "emw.Ex");
    model.variable("var4").set("Ey", "emw.Ey");
    model.variable("var4").set("Ez", "emw.Ez");
    model.variable("var4").set("Hx", "emw.Hx");
    model.variable("var4").set("Hy", "emw.Hy");
    model.variable("var4").set("Hz", "emw.Hz");
    model.variable("var4").set("Dx", "emw.Dx");
    model.variable("var4").set("Dy", "emw.Dy");

    return model;
  }

  public static Model run4(Model model) {
    model.variable("var4").set("Dz", "emw.Dz");
    model.variable("var4").set("Bx", "emw.Bx");
    model.variable("var4").set("By", "emw.By");
    model.variable("var4").set("Bz", "emw.Bz");
    model.variable("var3").set("Exeff", "intdS(-Ey*nx*y+Ex*ny*y-Ez*nx*z+Ex*nz*z)/2/V");
    model.variable("var3").set("Eyeff", "intdS(Ey*nx*x-Ex*ny*x-Ez*ny*z+Ey*nz*z)/2/V");
    model.variable("var3").set("Ezeff", "intdS(Ez*nx*x-Ex*nz*x+Ez*ny*y-Ey*nz*y)/2/V");
    model.variable("var3").set("Hxeff", "intdS(-Hy*nx*y+Hx*ny*y-Hz*nx*z+Hx*nz*z)/2/V");
    model.variable("var3").set("Hyeff", "intdS(Hy*nx*x-Hx*ny*x-Hz*ny*z+Hy*nz*z)/2/V");
    model.variable("var3").set("Hzeff", "intdS(Hz*nx*x-Hx*nz*x+Hz*ny*y-Hy*nz*y)/2/V");
    model.variable("var3").set("Dxeff", "intdS(-Hz*ny+Hy*nz)/(-j*omega0*V)");
    model.variable("var3").set("Dyeff", "intdS(Hz*nx-Hx*nz)/(-j*omega0*V)");
    model.variable("var3").set("Dzeff", "intdS(-Hy*nx+Hx*ny)/(-j*omega0*V)");
    model.variable("var3").set("Bxeff", "intdS(-Ez*ny+Ey*nz)/(j*omega0*V)");
    model.variable("var3").set("Byeff", "intdS(Ez*nx-Ex*nz)/(j*omega0*V)");
    model.variable("var3").set("Bzeff", "intdS(-Ey*nx+Ex*ny)/(j*omega0*V)");

    model.material("mat2").propertyGroup("def").set("relpermeability", new String[]{"1"});

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.variable("var4").rename("Ex", "Exg");
    model.variable("var4").set("Exg", "emw.Ex");
    model.variable("var4").rename("Ey", "Eyg");
    model.variable("var4").set("Eyg", "emw.Ey");
    model.variable("var4").rename("Ez", "Ezg");
    model.variable("var4").set("Ezg", "emw.Ez");
    model.variable("var4").rename("Hx", "Hxg");
    model.variable("var4").set("Hxg", "emw.Hx");
    model.variable("var4").rename("Hy", "Hyg");
    model.variable("var4").set("Hyg", "emw.Hy");
    model.variable("var4").rename("Hz", "Hzg");
    model.variable("var4").set("Hzg", "emw.Hz");
    model.variable("var4").rename("Dx", "Dxg");
    model.variable("var4").set("Dxg", "emw.Dx");
    model.variable("var4").rename("Dy", "Dyg");
    model.variable("var4").set("Dyg", "emw.Dy");
    model.variable("var4").rename("Dz", "Dzg");
    model.variable("var4").set("Dzg", "emw.Dz");
    model.variable("var4").rename("Bx", "Bxg");
    model.variable("var4").set("Bxg", "emw.Bx");
    model.variable("var4").rename("By", "Byg");
    model.variable("var4").set("Byg", "emw.By");
    model.variable("var4").rename("Bz", "Bzg");
    model.variable("var4").set("Bzg", "emw.Bz");
    model.variable("var3").set("Exeff", "intdS(-Eyg*nx*y+Exg*ny*y-Ezg*nx*z+Exg*nz*z)/2/V");
    model.variable("var3").set("Eyeff", "intdS(Eyg*nx*x-Exg*ny*x-Ezg*ny*z+Eyg*nz*z)/2/V");
    model.variable("var3").set("Ezeff", "intdS(Ezg*nx*x-Exg*nz*x+Ezg*ny*y-Eyg*nz*y)/2/V");
    model.variable("var3").set("Hxeff", "intdS(-Hyg*nx*y+Hxg*ny*y-Hzg*nx*z+Hxg*nz*z)/2/V");
    model.variable("var3").set("Hyeff", "intdS(Hyg*nx*x-Hxg*ny*x-Hzg*ny*z+Hyg*nz*z)/2/V");
    model.variable("var3").set("Hzeff", "intdS(Hzg*nx*x-Hxg*nz*x+Hzg*ny*y-Hyg*nz*y)/2/V");
    model.variable("var3").set("Dxeff", "intdS(-Hzg*ny+Hyg*nz)/(-j*omega0*V)");
    model.variable("var3").set("Dyeff", "intdS(Hzg*nx-Hxg*nz)/(-j*omega0*V)");
    model.variable("var3").set("Dzeff", "intdS(-Hyg*nx+Hxg*ny)/(-j*omega0*V)");
    model.variable("var3").set("Bxeff", "intdS(-Ezg*ny+Eyg*nz)/(j*omega0*V)");
    model.variable("var3").set("Byeff", "intdS(Ezg*nx-Exg*nz)/(j*omega0*V)");
    model.variable("var3").set("Bzeff", "intdS(-Eyg*nx+Exg*ny)/(j*omega0*V)");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result().table().remove("tbl1");
    model.result().numerical("gev1").set("expr", "muxeff");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "muzeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();

    model.variable().create("var5");
    model.variable("var5").set("Exg", "emw.Ex");
    model.variable("var5").set("Eyg", "emw.Ey");
    model.variable("var5").set("Ezg", "emw.Ez");
    model.variable("var5").set("Hxg", "emw.Hx");
    model.variable("var5").set("Hyg", "emw.Hy");
    model.variable("var5").set("Hzg", "emw.Hz");
    model.variable("var5").set("Dxg", "emw.Dx");
    model.variable("var5").set("Dyg", "emw.Dy");
    model.variable("var5").set("Dzg", "emw.Dz");
    model.variable("var5").set("Bxg", "emw.Bx");
    model.variable("var5").set("Byg", "emw.By");
    model.variable("var5").set("Bzg", "emw.Bz");
    model.variable("var5").remove("Exg");
    model.variable("var5").remove("Eyg");
    model.variable("var5").remove("Ezg");
    model.variable("var5").remove("Hxg");
    model.variable("var5").remove("Hyg");
    model.variable("var5").remove("Hzg");
    model.variable("var5").remove("Dxg");
    model.variable("var5").remove("Dyg");
    model.variable("var5").remove("Dzg");
    model.variable("var5").remove("Bxg");
    model.variable("var5").remove("Byg");
    model.variable("var5").remove("Bzg");
    model.variable("var5").set("Exeff", "intdS(-Eyg*nx*y+Exg*ny*y-Ezg*nx*z+Exg*nz*z)/2/V");
    model.variable("var5").set("Eyeff", "intdS(Eyg*nx*x-Exg*ny*x-Ezg*ny*z+Eyg*nz*z)/2/V");
    model.variable("var5").set("Ezeff", "intdS(Ezg*nx*x-Exg*nz*x+Ezg*ny*y-Eyg*nz*y)/2/V");
    model.variable("var5").set("Hxeff", "intdS(-Hyg*nx*y+Hxg*ny*y-Hzg*nx*z+Hxg*nz*z)/2/V");
    model.variable("var5").set("Hyeff", "intdS(Hyg*nx*x-Hxg*ny*x-Hzg*ny*z+Hyg*nz*z)/2/V");
    model.variable("var5").set("Hzeff", "intdS(Hzg*nx*x-Hxg*nz*x+Hzg*ny*y-Hyg*nz*y)/2/V");
    model.variable("var5").set("Dxeff", "intdS(-Hzg*ny+Hyg*nz)/(-j*omega0*V)");
    model.variable("var5").set("Dyeff", "intdS(Hzg*nx-Hxg*nz)/(-j*omega0*V)");
    model.variable("var5").set("Dzeff", "intdS(-Hyg*nx+Hxg*ny)/(-j*omega0*V)");
    model.variable("var5").set("Bxeff", "intdS(-Ezg*ny+Eyg*nz)/(j*omega0*V)");
    model.variable("var5").set("Byeff", "intdS(Ezg*nx-Exg*nz)/(j*omega0*V)");
    model.variable("var5").set("Bzeff", "intdS(-Eyg*nx+Exg*ny)/(j*omega0*V)");
    model.variable("var5").set("muxeff", "Bxeff/Hxeff/mu0");
    model.variable("var5").set("muyeff", "Byeff/Hyeff/mu0");
    model.variable("var5").set("muzeff", "Bzeff/Hzeff/mu0");
    model.variable("var5").set("epsxeff", "Dxeff/Exeff/eps0");
    model.variable("var5").set("epsyeff", "Dyeff/Eyeff/eps0");
    model.variable("var5").set("epszeff", "Dzeff/Ezeff/eps0");
    model.variable("var5").set("Exeff", "comp1.intdS(-Eyg*nx*y+Exg*ny*y-Ezg*nx*z+Exg*nz*z)/2/V");
    model.variable("var5").set("Eyeff", "comp1.intdS(Eyg*nx*x-Exg*ny*x-Ezg*ny*z+Eyg*nz*z)/2/V");
    model.variable("var5").set("Ezeff", "comp1.intdS(Ezg*nx*x-Exg*nz*x+Ezg*ny*y-Eyg*nz*y)/2/V");
    model.variable("var5").set("Hxeff", "comp1.intdS(-Hyg*nx*y+Hxg*ny*y-Hzg*nx*z+Hxg*nz*z)/2/V");
    model.variable("var5").set("Hyeff", "comp1.intdS(Hyg*nx*x-Hxg*ny*x-Hzg*ny*z+Hyg*nz*z)/2/V");
    model.variable("var5").set("Hzeff", "comp1.intdS(Hzg*nx*x-Hxg*nz*x+Hzg*ny*y-Hyg*nz*y)/2/V");
    model.variable("var5").set("Dxeff", "comp1.intdS(-Hzg*ny+Hyg*nz)/(-j*omega0*V)");
    model.variable("var5").set("Dyeff", "comp1.intdS(Hzg*nx-Hxg*nz)/(-j*omega0*V)");
    model.variable("var5").set("Dzeff", "comp1.intdS(-Hyg*nx+Hxg*ny)/(-j*omega0*V)");
    model.variable("var5").set("Bxeff", "comp1.intdS(-Ezg*ny+Eyg*nz)/(j*omega0*V)");
    model.variable("var5").set("Byeff", "comp1.intdS(Ezg*nx-Exg*nz)/(j*omega0*V)");
    model.variable("var5").set("Bzeff", "comp1.intdS(-Eyg*nx+Exg*ny)/(j*omega0*V)");
    model.variable().remove("var3");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.variable("var4").rename("Exg", "Ex");
    model.variable("var4").set("Ex", "emw.Ex");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.variable("var5").set("Exeff", "comp1.intdS(-Eyg*nx*y+Ex*ny*y-Ezg*nx*z+Ex*nz*z)/2/V");
    model.variable("var5").set("Eyeff", "comp1.intdS(Eyg*nx*x-Ex*ny*x-Ezg*ny*z+Eyg*nz*z)/2/V");
    model.variable("var5").set("Ezeff", "comp1.intdS(Ezg*nx*x-Ex*nz*x+Ezg*ny*y-Eyg*nz*y)/2/V");
    model.variable("var5").set("Byeff", "comp1.intdS(Ezg*nx-Ex*nz)/(j*omega0*V)");
    model.variable("var5").set("Bzeff", "comp1.intdS(-Eyg*nx+Ex*ny)/(j*omega0*V)");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.variable("var4").rename("Ex", "Exg");
    model.variable("var4").set("Exg", "emw.Ex");
    model.variable("var4").rename("Exg", "ex");
    model.variable("var4").set("ex", "emw.Ex");
    model.variable("var5").set("Exeff", "comp1.intdS(-Eyg*nx*y+ex*ny*y-Ezg*nx*z+ex*nz*z)/2/V");
    model.variable("var5").set("Eyeff", "comp1.intdS(Eyg*nx*x-ex*ny*x-Ezg*ny*z+Eyg*nz*z)/2/V");
    model.variable("var5").set("Ezeff", "comp1.intdS(Ezg*nx*x-ex*nz*x+Ezg*ny*y-Eyg*nz*y)/2/V");
    model.variable("var5").set("Byeff", "comp1.intdS(Ezg*nx-ex*nz)/(j*omega0*V)");
    model.variable("var5").set("Bzeff", "comp1.intdS(-Eyg*nx+ex*ny)/(j*omega0*V)");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result().table().remove("tbl1");
    model.result().numerical("gev1").set("expr", "muxeff");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "muzeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();

    model.variable("var4").rename("Eyg", "ey");
    model.variable("var4").set("ey", "emw.Ey");
    model.variable("var4").rename("Ezg", "ez");
    model.variable("var4").set("ez", "emw.Ez");
    model.variable("var4").rename("Hxg", "hx");
    model.variable("var4").set("hx", "emw.Hx");
    model.variable("var4").rename("Hyg", "hy");
    model.variable("var4").set("hy", "emw.Hy");
    model.variable("var4").rename("Hzg", "hz");
    model.variable("var4").set("hz", "emw.Hz");
    model.variable("var4").rename("Dxg", "dx");
    model.variable("var4").set("dx", "emw.Dx");
    model.variable("var4").rename("Dyg", "dy");
    model.variable("var4").set("dy", "emw.Dy");
    model.variable("var4").rename("Dzg", "dz");
    model.variable("var4").set("dz", "emw.Dz");
    model.variable("var4").rename("Bxg", "bx");
    model.variable("var4").set("bx", "emw.Bx");
    model.variable("var4").rename("Byg", "by");
    model.variable("var4").set("by", "emw.By");
    model.variable("var4").rename("Bzg", "bz");
    model.variable("var4").set("bz", "emw.Bz");
    model.variable("var5").set("Exeff", "comp1.intdS(-ey*nx*y+ex*ny*y-ez*nx*z+ex*nz*z)/2/V");
    model.variable("var5").set("Eyeff", "comp1.intdS(ey*nx*x-ex*ny*x-ez*ny*z+ey*nz*z)/2/V");
    model.variable("var5").set("Ezeff", "comp1.intdS(ez*nx*x-ex*nz*x+ez*ny*y-ey*nz*y)/2/V");
    model.variable("var5").set("Hxeff", "comp1.intdS(-hy*nx*y+hx*ny*y-hz*nx*z+hx*nz*z)/2/V");
    model.variable("var5").set("Hyeff", "comp1.intdS(hy*nx*x-hx*ny*x-hz*ny*z+hy*nz*z)/2/V");
    model.variable("var5").set("Hzeff", "comp1.intdS(hz*nx*x-hx*nz*x+hz*ny*y-hy*nz*y)/2/V");
    model.variable("var5").set("Dxeff", "comp1.intdS(-hz*ny+hy*nz)/(-j*omega0*V)");
    model.variable("var5").set("Dyeff", "comp1.intdS(hz*nx-hx*nz)/(-j*omega0*V)");
    model.variable("var5").set("Dzeff", "comp1.intdS(-hy*nx+hx*ny)/(-j*omega0*V)");
    model.variable("var5").set("Bxeff", "comp1.intdS(-ez*ny+ey*nz)/(j*omega0*V)");
    model.variable("var5").set("Byeff", "comp1.intdS(ez*nx-ex*nz)/(j*omega0*V)");
    model.variable("var5").set("Bzeff", "comp1.intdS(-ey*nx+ex*ny)/(j*omega0*V)");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result().table().remove("tbl1");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muzeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "muxeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();

    model.variable("var1").label("Local Field");
    model.variable("var5").label("Effective Medium");
    model.variable("var4").label("Variables 2");
    model.variable("var4").label("Microscopic Fields");
    model.variable("var1").label("Local Fields");

    model.material("mat2").propertyGroup("def").set("relpermeability", new String[]{"-1000"});

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();

    model.material("mat2").propertyGroup("def").set("relpermeability", new String[]{"-100"});

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");

    return model;
  }

  public static Model run5(Model model) {
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result().table().remove("tbl1");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "muzeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();

    model.label("template_Hloc3d_v50.mph");

    model.param().set("epsm", "1");
    model.param().set("mum", "-100");

    model.material("mat2").propertyGroup("def").set("relpermittivity", new String[]{"epsm"});
    model.material("mat2").propertyGroup("def").set("relpermeability", new String[]{"mum"});

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "hx");
    model.result("pg1").run();
    model.result().table().remove("tbl1");
    model.result().numerical("gev1").set("expr", "muxeff");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "muzeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();

    model.label("template_Hloc3d_v50.mph");

    model.param().set("epsm", "-100");
    model.param().set("mum", "1");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "hy");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "hz");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "hx");
    model.result("pg1").run();
    model.result().table().remove("tbl1");
    model.result().numerical("gev1").set("expr", "muxeff");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "muzeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();

    model.label("sphere_Hloc3d_v50.mph");

    model.geom("geom1").feature("cyl1").set("h", "a3/2");
    model.geom("geom1").feature("cyl1").set("pos", new String[]{"0", "0", "-a3/4"});
    model.geom("geom1").run("fin");

    model.param().rename("Rcyl", "Rsph");
    model.param().set("Rsph", "1.5e-4[m]");
    model.param().rename("Acyl", "Vcyl");
    model.param().set("Vcyl", "4*pi/3*Rsph^3");
    model.param().rename("Vcyl", "Vsph");
    model.param().set("Vsph", "4*pi/3*Rsph^3");

    model.geom("geom1").run("blk1");
    model.geom("geom1").feature().remove("cyl1");
    model.geom("geom1").run("blk1");
    model.geom("geom1").create("sph1", "Sphere");
    model.geom("geom1").feature("sph1").set("r", "Rsph");
    model.geom("geom1").run("sph1");
    model.geom("geom1").run();

    model.cpl("intop1").selection().set(new int[]{1, 2, 3, 4, 5, 14});

    model.param().set("epsm", "1");

    model.geom("geom1").run("fin");

    model.mesh("mesh1").run();

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "hy");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "hz");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "hx");
    model.result("pg1").run();
    model.result().numerical("gev1").set("expr", "muxeff");
    model.result().table().remove("tbl1");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "muzeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();

    model.param().set("epsm", "5");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");
    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "hy");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "hz");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "hx");
    model.result("pg1").run();
    model.result().table().remove("tbl1");
    model.result().numerical("gev1").set("expr", "muxeff");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "muzeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();

    model.label("SRR_Hloc3d_v50.mph");

    model.param().set("a1", "5.25[mm]");
    model.param().set("Rsph", "1.5[mm]");
    model.param().remove("Vsph");

    model.geom("geom1").run("fin");
    model.geom("geom1").feature().remove("sph1");
    model.geom("geom1").run("fin");

    model.param().set("tmetal", "0.1[mm]");
    model.param().remove("Rsph");

    model.geom("geom1").feature().duplicate("blk2", "blk1");
    model.geom("geom1").feature("blk2").set("size", new String[]{"a1", "tmetal", "a3"});
    model.geom("geom1").run("blk2");

    model.param().set("wring", "3.75[mm]");
    model.param().set("wring2", "3[mm]");

    model.geom("geom1").feature("blk2").set("size", new String[]{"wring", "tmetal", "a3"});
    model.geom("geom1").feature("blk2").setIndex("size", "wring", 2);
    model.geom("geom1").run("blk2");
    model.geom("geom1").feature().duplicate("blk3", "blk2");
    model.geom("geom1").feature("blk2").setIndex("size", "a1", 0);
    model.geom("geom1").feature("blk2").set("size", new String[]{"a1", "tmetal", "a3"});
    model.geom("geom1").run("blk2");
    model.geom("geom1").run("blk3");

    model.param().set("wmetal", "0.25[mm]");

    model.geom("geom1").run("blk3");
    model.geom("geom1").feature().duplicate("blk4", "blk3");
    model.geom("geom1").feature("blk4").setIndex("size", "wring-2*wmetal", 0);
    model.geom("geom1").feature("blk4").setIndex("size", "wring-2*wmetal", 2);
    model.geom("geom1").run("blk4");
    model.geom("geom1").feature().duplicate("blk5", "blk3");
    model.geom("geom1").feature("blk5").setIndex("size", "wring2", 0);
    model.geom("geom1").feature("blk5").setIndex("size", "wring2", 2);
    model.geom("geom1").run("blk5");
    model.geom("geom1").feature().duplicate("blk6", "blk5");
    model.geom("geom1").feature("blk6").setIndex("size", "wring2-2*wmetal", 0);
    model.geom("geom1").feature("blk6").setIndex("size", "wring2-2*wmetal", 2);
    model.geom("geom1").run("blk6");

    model.param().set("gapsize", "0.125[mm]");

    model.geom("geom1").feature().duplicate("blk7", "blk6");
    model.geom("geom1").feature("blk7").setIndex("size", "gapsize", 0);
    model.geom("geom1").feature("blk7").setIndex("size", "wmetal", 2);
    model.geom("geom1").feature("blk7").set("pos", new String[]{"0", "0", "wring/2-wmetal/2"});
    model.geom("geom1").run("blk7");
    model.geom("geom1").run("blk7");
    model.geom("geom1").feature().duplicate("blk8", "blk7");
    model.geom("geom1").feature("blk8").set("pos", new String[]{"0", "0", "wring2/2+wmetal/2"});
    model.geom("geom1").run("blk8");
    model.geom("geom1").feature("blk8").set("pos", new String[]{"0", "0", "-wring2/2+wmetal/2"});
    model.geom("geom1").run("blk8");
    model.geom("geom1").run("fin");

    model.material("mat2").propertyGroup("def").set("relpermittivity", new String[]{"epsCu"});
    model.material("mat2").propertyGroup("def").set("relpermeability", new String[]{"1"});

    model.param().remove("epsm");
    model.param().remove("mum");
    model.param().set("epsCu", "1-i*sigmaCu/(eps0*2*pi*freq)");
    model.param().set("sigmaCu", "5.8e7[S/m]");
    model.param().set("epsCu", "1-i*sigmaCu/(eps0*2*pi*freq0)");

    model.material("mat2").selection().set(new int[]{});
    model.material("mat2").selection().all();
    model.material("mat2").selection().set(new int[]{4, 6});
    model.material("mat1").selection().all();
    model.material("mat1").selection().set(new int[]{1, 2, 3, 5, 7, 8, 9});

    model.mesh("mesh1").run("edg1");

    model.param().set("meshsize", "1[mm]");

    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").run("cpe1");
    model.mesh("mesh1").run("cpe2");
    model.mesh("mesh1").run("cpe3");
    model.mesh("mesh1").run("ftri1");
    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").run("cpe1");
    model.mesh("mesh1").run("cpe1");
    model.mesh("mesh1").run("cpe1");
    model.mesh("mesh1").run("cpe2");
    model.mesh("mesh1").feature().duplicate("cpe4", "cpe3");
    model.mesh("mesh1").feature().move("cpe3", 4);
    model.mesh("mesh1").feature().duplicate("cpe5", "cpe4");
    model.mesh("mesh1").feature("cpe3").selection("source").remove(new int[]{7});
    model.mesh("mesh1").feature("cpe3").selection("source").remove(new int[]{12});
    model.mesh("mesh1").feature("cpe3").selection("destination").remove(new int[]{9});
    model.mesh("mesh1").feature("cpe3").selection("destination").remove(new int[]{14});
    model.mesh("mesh1").feature("cpe3").selection("destination").remove(new int[]{103});
    model.mesh("mesh1").feature("cpe3").selection("destination").remove(new int[]{106});
    model.mesh("mesh1").run("cpe3");
    model.mesh("mesh1").feature("cpe4").selection("source").remove(new int[]{2});
    model.mesh("mesh1").feature("cpe4").selection("source").remove(new int[]{12});
    model.mesh("mesh1").feature("cpe4").selection("destination").remove(new int[]{4});
    model.mesh("mesh1").feature("cpe4").selection("destination").remove(new int[]{14});
    model.mesh("mesh1").feature("cpe4").selection("destination").remove(new int[]{100});
    model.mesh("mesh1").feature("cpe4").selection("destination").remove(new int[]{106});
    model.mesh("mesh1").run("cpe4");
    model.mesh("mesh1").feature("cpe5").selection("source").remove(new int[]{2});
    model.mesh("mesh1").feature("cpe5").selection("source").remove(new int[]{7});
    model.mesh("mesh1").feature("cpe5").selection("destination").remove(new int[]{4});
    model.mesh("mesh1").feature("cpe5").selection("destination").remove(new int[]{9});
    model.mesh("mesh1").feature("cpe5").selection("destination").remove(new int[]{100});
    model.mesh("mesh1").feature("cpe5").selection("destination").remove(new int[]{103});
    model.mesh("mesh1").run("cpe5");
    model.mesh("mesh1").run("ftri1");
    model.mesh("mesh1").run("cpf1");
    model.mesh("mesh1").run("cpf2");
    model.mesh("mesh1").run("cpf3");
    model.mesh("mesh1").run("ftet1");

    model.param().set("freq0", "7[GHz]");

    model.sol("sol1").runAll();

    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "ez");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "ex");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "sqrt(abs(ex)^2+abs(ey)^2+abs(ez)^2)");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "hy");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "sqrt(abs(ex)^2+abs(ey)^2+abs(ez)^2)");
    model.result("pg1").run();
    model.result().table().remove("tbl1");
    model.result().numerical("gev1").set("expr", "muxeff");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().numerical("gev1").set("expr", "muzeff");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();

    model.variable("var1").set("Bxloc", "0.0[A/m]*mu0");
    model.variable("var1").set("Byloc", "1.0[A/m]*mu0");
    model.variable("var1").set("Bzloc", "0.0[A/m]*mu0");

    model.study("std1").feature("param").active(true);
    model.study("std1").feature("param").setIndex("plistarr", "range(5[GHz],10[GHz],1[GHz])", 0);

    model.batch().create("p1", "Parametric");
    model.batch("p1").study("std1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "none");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(5[GHz],10[GHz],1[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").set("pdistrib", "off");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");

    model.sol().create("sol2");
    model.sol("sol2").study("std1");
    model.sol("sol2").label("Parametric Solutions 1");

    model.batch("p1").feature("so1").set("psol", "sol2");

    model.result().create("pg2", "PlotGroup3D");
    model.result("pg2").label("Electric Field (emw) 1");
    model.result("pg2").set("data", "dset2");
    model.result("pg2").set("oldanalysistype", "noneavailable");
    model.result("pg2").set("frametype", "spatial");
    model.result("pg2").set("data", "dset2");
    model.result("pg2").feature().create("mslc1", "Multislice");
    model.result("pg2").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg2").feature("mslc1").set("data", "parent");

    model.study("std1").feature("param").setIndex("plistarr", "range(5[GHz],1[GHz],10[GHz])", 0);

    model.batch("p1").run();

    model.result("pg2").run();
    model.result("pg2").run();
    model.result("pg2").run();
    model.result("pg2").run();
    model.result("pg2").set("looplevel", new String[]{"1", "1"});
    model.result("pg2").run();
    model.result().create("pg3", "PlotGroup1D");
    model.result("pg3").run();
    model.result("pg3").create("glob1", "Global");
    model.result("pg3").feature("glob1").setIndex("expr", "myueff", 0);
    model.result("pg3").feature("glob1").setIndex("expr", "muyeff", 0);
    model.result("pg3").run();
    model.result("pg3").feature("glob1").set("data", "dset2");
    model.result("pg3").feature("glob1").setIndex("looplevelinput", "first", 0);
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("looplevelinput", "all", 0);
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("looplevelinput", "first", 1);
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("looplevelinput", "all", 1);
    model.result("pg3").feature("glob1").set("xdatasolnumtype", "all");
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("looplevelinput", "first", 0);
    model.result("pg3").feature("glob1").set("xdatasolnumtype", "inner");
    model.result("pg3").run();
    model.result("pg3").feature("glob1").set("xdatasolnumtype", "outer");
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("looplevelinput", "all", 0);
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("expr", "imag(muyeff)", 0);
    model.result("pg3").run();
    model.result("pg3").run();
    model.result().table().remove("tbl1");
    model.result("pg3").run();
    model.result("pg3").run();
    model.result().table().create("tbl1", "Table");
    model.result().dataset().remove("dset1");
    model.result().dataset().remove("dset2");
    model.result().table().remove("tbl1");

    model.study("std1").feature("param").setIndex("plistarr", "range(5[GHz],0.2[GHz],10[GHz])", 0);

    model.result().dataset().create("dset1", "Solution");
    model.result().dataset("dset1").set("solution", "sol2");
    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("outersolnum", 1);
    model.result("pg1").set("solvertype", "none");
    model.result("pg1").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("outertype", "none");
    model.result("pg1").feature("mslc1").set("solvertype", "none");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result().create("pg2", "PlotGroup1D");
    model.result("pg2").run();
    model.result("pg2").create("glob1", "Global");
    model.result("pg2").feature("glob1").setIndex("expr", "muyeff", 0);
    model.result("pg2").run();
    model.result("pg2").feature("glob1").set("data", "dset1");
    model.result("pg2").feature("glob1").set("xdata", "spectrum");
    model.result("pg2").run();
    model.result("pg2").feature("glob1").set("xdata", "solution");
    model.result("pg2").feature("glob1").setIndex("looplevelinput", "first", 0);
    model.result("pg2").feature("glob1").set("xdatasolnumtype", "outer");
    model.result("pg2").feature("glob1").set("data", "parent");
    model.result("pg2").run();
    model.result().export().create("data1", "Data");
    model.result("pg2").run();
    model.result("pg2").feature("glob1").setIndex("expr", "imag(muyeff)", 1);
    model.result("pg2").run();
    model.result().export("data1").setIndex("expr", "muyeff", 0);
    model.result().export("data1").set("filename", "C:\\Users\\jensen\\Downloads\\fieldavg\\SRR_muyeff.txt");
    model.result().export("data1").run();
    model.result("pg2").run();
    model.result("pg2").feature().remove("glob1");
    model.result("pg2").run();
    model.result("pg2").create("lngr1", "LineGraph");
    model.result("pg2").feature().remove("lngr1");
    model.result("pg2").run();
    model.result("pg2").create("lngr1", "LineGraph");
    model.result("pg2").feature().remove("lngr1");
    model.result("pg2").run();
    model.result("pg2").create("glob1", "Global");
    model.result("pg2").feature("glob1").setIndex("expr", "muyeff", 0);
    model.result("pg2").feature("glob1").set("xdatasolnumtype", "outer");
    model.result("pg2").run();
    model.result().export().remove("data1");
    model.result().export().create("plot1", "Plot");
    model.result("pg1").set("window", "graphics");
    model.result("pg1").run();
    model.result("pg1").set("window", "graphics");
    model.result("pg1").set("windowtitle", "");
    model.result().export("plot1").set("plotgroup", "pg2");
    model.result().export("plot1").set("filename", "C:\\Users\\jensen\\Downloads\\fieldavg\\SRR_muyeff.txt");
    model.result("pg2").set("window", "graphics");
    model.result("pg2").run();
    model.result("pg2").set("window", "graphics");
    model.result("pg2").set("windowtitle", "");
    model.result("pg2").set("window", "graphics");
    model.result("pg2").run();
    model.result("pg2").set("window", "graphics");
    model.result("pg2").set("windowtitle", "");
    model.result().export("plot1").run();
    model.result("pg1").run();
    model.result("pg1").run();
    model.result("pg2").set("window", "graphics");
    model.result("pg2").run();
    model.result("pg2").set("window", "graphics");
    model.result("pg2").set("windowtitle", "");
    model.result("pg2").set("window", "graphics");
    model.result("pg2").run();
    model.result("pg2").set("window", "graphics");
    model.result("pg2").set("windowtitle", "");
    model.result("pg2").run();
    model.result("pg2").run();
    model.result("pg2").run();
    model.result("pg2").run();
    model.result("pg2").setIndex("looplevelinput", "manualindices", 1);
    model.result("pg2").setIndex("looplevelinput", "all", 1);
    model.result("pg2").setIndex("looplevelinput", "manualindices", 0);
    model.result("pg2").setIndex("looplevelinput", "all", 0);
    model.result("pg2").setIndex("looplevelinput", "manual", 1);
    model.result("pg2").setIndex("looplevel", "11", 1);
    model.result("pg2").run();
    model.result("pg2").setIndex("looplevelinput", "all", 1);
    model.result("pg2").run();
    model.result("pg2").run();

    model.study("std1").feature("param").setIndex("plistarr", "range(5[GHz],5[GHz],10[GHz])", 0);

    model.result().dataset().remove("dset1");

    model.variable("var1").set("Bxloc", "1.0[A/m]*mu0");
    model.variable("var1").set("Byloc", "0.0[A/m]*mu0");

    model.result().dataset().create("dset1", "Solution");
    model.result().dataset("dset1").set("solution", "sol2");
    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");

    return model;
  }

  public static Model run6(Model model) {
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("outersolnum", 1);
    model.result("pg1").set("solvertype", "none");
    model.result("pg1").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("outertype", "none");
    model.result("pg1").feature("mslc1").set("solvertype", "none");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result().numerical().create("gev1", "EvalGlobal");
    model.result().numerical("gev1").set("expr", "muxeff");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "ex");
    model.result("pg1").run();
    model.result("pg1").feature("mslc1").set("expr", "ey");
    model.result("pg1").run();

    model.param().set("meshsize", "3[mm]");

    model.mesh("mesh1").run();

    model.result().dataset().remove("dset1");
    model.result().dataset().create("dset1", "Solution");
    model.result().dataset("dset1").set("solution", "sol2");
    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("outersolnum", 1);
    model.result("pg1").set("solvertype", "none");
    model.result("pg1").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("outertype", "none");
    model.result("pg1").feature("mslc1").set("solvertype", "none");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.param().set("tmetal", "0.5[mm]");

    model.geom("geom1").run("fin");

    model.param().set("tmetal", "0.25[mm]");

    model.geom("geom1").run("fin");

    model.param().set("meshsize", "1[mm]");

    model.mesh("mesh1").run();

    model.result().dataset().remove("dset1");
    model.result().dataset().create("dset1", "Solution");
    model.result().dataset("dset1").set("solution", "sol2");
    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result().table().remove("tbl1");
    model.result().numerical().create("gev1", "EvalGlobal");
    model.result().numerical("gev1").set("expr", "muxeff");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muxeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();

    model.variable("var1").set("Bxloc", "0.0[A/m]*mu0");
    model.variable("var1").set("Byloc", "1.0[A/m]*mu0");

    model.result().dataset().remove("dset1");
    model.result().dataset().create("dset1", "Solution");
    model.result().dataset("dset1").set("solution", "sol2");
    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("outersolnum", 1);
    model.result("pg1").set("solvertype", "none");
    model.result("pg1").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("outertype", "none");
    model.result("pg1").feature("mslc1").set("solvertype", "none");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result().numerical().create("gev1", "EvalGlobal");
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().table().create("tbl2", "Table");
    model.result().table("tbl2").comments("Global Evaluation 1 (muyeff)");
    model.result().numerical("gev1").set("table", "tbl2");
    model.result().numerical("gev1").setResult();
    model.result().table().remove("tbl2");
    model.result().table().remove("tbl1");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muyeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("tablecols", "outer");
    model.result().table().remove("tbl1");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muyeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("tablecols", "data");
    model.result().table().remove("tbl1");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muyeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().dataset().remove("dset1");

    model.param().set("meshsize", "2[mm]");

    model.mesh("mesh1").run();

    model.result().dataset().create("dset1", "Solution");
    model.result().dataset("dset1").set("solution", "sol2");
    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("outersolnum", 1);
    model.result("pg1").set("solvertype", "none");
    model.result("pg1").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("outertype", "none");
    model.result("pg1").feature("mslc1").set("solvertype", "none");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result().table().remove("tbl1");
    model.result().numerical().create("gev1", "EvalGlobal");
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (muyeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().dataset().remove("dset1");

    model.study("std1").feature("param").setIndex("plistarr", "range(5[GHz],0.1[GHz],10[GHz])", 0);

    model.result().dataset().create("dset1", "Solution");
    model.result().dataset("dset1").set("solution", "sol2");
    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("outersolnum", 1);
    model.result("pg1").set("solvertype", "none");
    model.result("pg1").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("oldouteranalysistype", "noneavailable");
    model.result("pg1").set("outertype", "none");
    model.result("pg1").set("data", "dset1");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("outertype", "none");
    model.result("pg1").feature("mslc1").set("solvertype", "none");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result().numerical().create("gev1", "EvalGlobal");
    model.result().numerical("gev1").set("expr", "muyeff");
    model.result().table().create("tbl2", "Table");
    model.result().table("tbl2").comments("Global Evaluation 1 (muyeff)");
    model.result().numerical("gev1").set("table", "tbl2");
    model.result().numerical("gev1").setResult();
    model.result().table("tbl2").save("C:\\Users\\jensen\\Downloads\\fieldavg\\SRR_muyeff.txt");
    model.result().numerical("gev1").set("expr", "real(muyeff)");
    model.result().table().remove("tbl2");
    model.result().table().remove("tbl1");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1 (real(muyeff))");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("expr", "imag(muyeff)");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").appendResult();
    model.result().table("tbl1").save("C:\\Users\\jensen\\Downloads\\fieldavg\\SRR_muyeff.dat");
    model.result().create("pg2", "PlotGroup1D");
    model.result("pg2").run();
    model.result("pg2").create("glob1", "Global");
    model.result("pg2").feature("glob1").setIndex("expr", "real(muyeff)", 0);
    model.result("pg2").feature("glob1").setIndex("expr", "imag(mueff)", 1);
    model.result("pg2").feature("glob1").setIndex("expr", "imag(muyeff)", 1);
    model.result("pg2").run();
    model.result("pg2").run();
    model.result("pg2").feature("glob1").set("data", "dset1");
    model.result("pg2").run();
    model.result("pg2").run();
    model.result("pg2").run();
    model.result("pg2").feature("glob1").set("xdatasolnumtype", "inner");
    model.result("pg2").feature("glob1").set("xdata", "solution");
    model.result("pg2").feature("glob1").set("xdatasolnumtype", "outer");
    model.result("pg2").run();
    model.result().table().remove("tbl1");
    model.result().export().create("plot1", "pg2", "glob1", "Plot");
    model.result("pg2").set("window", "graphics");
    model.result("pg2").run();
    model.result("pg2").set("window", "graphics");
    model.result("pg2").set("windowtitle", "");
    model.result().export("plot1").set("filename", "C:\\Users\\jensen\\Downloads\\fieldavg\\SRR_muyeff.dat");

    model.label("SRR_Hloc3d_v50.mph");

    model.result("pg2").run();
    model.result("pg2").set("window", "graphics");
    model.result("pg2").run();
    model.result("pg2").set("window", "graphics");
    model.result("pg2").set("windowtitle", "");
    model.result().export().remove("plot1");
    model.result("pg2").run();
    model.result().export().create("plot1", "pg2", "glob1", "Plot");
    model.result("pg2").set("window", "graphics");
    model.result("pg2").run();
    model.result("pg2").set("window", "graphics");
    model.result("pg2").set("windowtitle", "");
    model.result().export("plot1").set("filename", "C:\\Users\\jensen\\Downloads\\fieldavg\\SRR_Hloc3d_v50.dat");
    model.result().export("plot1").run();

    model.label("SRR_Hloc3d_v50.mph");

    model.result("pg2").run();

    model.study("std1").feature("param").setIndex("plistarr", "range(6.1[GHz],0.1[GHz],6.2[GHz])", 0);

    model.result().dataset().create("dset2", "Solution");
    model.result().dataset("dset2").set("solution", "sol1");
    model.result().create("pg3", "PlotGroup3D");
    model.result("pg3").label("Electric Field (emw) 1");
    model.result("pg3").set("data", "dset2");
    model.result("pg3").set("oldanalysistype", "noneavailable");
    model.result("pg3").set("solvertype", "none");
    model.result("pg3").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg3").set("frametype", "spatial");
    model.result("pg3").set("data", "dset2");
    model.result("pg3").feature().create("mslc1", "Multislice");
    model.result("pg3").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg3").feature("mslc1").set("solvertype", "none");
    model.result("pg3").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg2").run();

    model.param().set("epsCu", "1-1e8*i");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg2").run();

    model.param().set("epsCu", "-1000-1e8*i");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg2").run();

    model.label("SRR_Hloc3d_v50.mph");

    model.study("std1").feature("param").setIndex("plistarr", "range(5.00[GHz],0.05[GHz],7.00[GHz])", 0);

    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg2").run();
    model.result("pg2").set("window", "graphics");
    model.result("pg2").run();
    model.result("pg2").set("window", "graphics");
    model.result("pg2").set("windowtitle", "");
    model.result().export("plot1").run();

    model.label("SRR_Hloc3d_v50.mph");

    model.result("pg2").set("window", "graphics");
    model.result("pg2").run();
    model.result("pg2").set("window", "graphics");
    model.result("pg2").set("windowtitle", "");

    model.geom("geom1").run("fin");
    model.geom("geom1").run("blk8");
    model.geom("geom1").create("blk9", "Block");
    model.geom("geom1").feature("blk9").set("size", new String[]{"3*a1", "a2", "a3"});
    model.geom("geom1").feature("blk9").set("base", "center");
    model.geom("geom1").run("blk9");

    model.variable().create("var6");
    model.variable("var6").set("Exin", "0[V/m]");
    model.variable("var6").set("Eyin", "0[V/m]");
    model.variable("var6").set("Ezin", "exp(-j*k0*x)*1[V/m]");
    model.variable().remove("var1");
    model.variable().remove("var5");
    model.variable().create("var7");
    model.variable("var7").set("e1", "comp1.intop1(emw.Ez)");
    model.variable("var7").set("h1", "-comp1.intop1(emw.Hy)");
    model.variable("var7").set("e2", "comp1.intop2(emw.Ez)");
    model.variable("var7").set("h2", "-comp1.intop2(emw.Hy)");
    model.variable("var7").set("ep1", "(e1+h1*eta0)/2");
    model.variable("var7").set("en1", "(e1-h1*eta0)/2");
    model.variable("var7").set("ep2", "(e2+h2*eta0)/2");
    model.variable("var7").set("en2", "(e2-h2*eta0)/2");
    model.variable("var7").set("tf", "ep2/ep1");
    model.variable("var7").set("rf", "en1/ep1");
    model.variable("var7").set("T", "abs(tf)^2");
    model.variable("var7").set("R", "abs(rf)^2");
    model.variable("var7").set("A", "1-T-R");

    model.geom("geom1").run();

    model.variable().remove("var4");

    model.cpl().remove("intop1");
    model.cpl().create("intop1", "Integration", "geom1");
    model.cpl("intop1").selection().geom("geom1", 2);
    model.cpl("intop1").selection().set(new int[]{});
    model.cpl("intop1").selection().all();
    model.cpl("intop1").selection().set(new int[]{6, 10, 14});
    model.cpl().create("intop2", "Integration", "geom1");
    model.cpl("intop2").selection().geom("geom1", 2);
    model.cpl("intop2").selection().all();
    model.cpl("intop2").selection().set(new int[]{59, 63, 64});

    model.material("mat1").selection().set(new int[]{1, 2, 3, 4, 6, 8, 9, 10, 11});

    model.physics("emw").prop("BackgroundField").set("Eb", new String[]{"Exin", "Eyin", "Ezin"});
    model.physics("emw").feature().remove("pc1");
    model.physics("emw").feature("pc2").selection().set(new int[]{2, 5, 7, 18, 60, 65});
    model.physics("emw").feature("pc3").selection().set(new int[]{3, 4, 8, 9, 12, 13, 16, 17, 61, 62});
    model.physics("emw").feature().create("sctr1", "Scattering", 2);
    model.physics("emw").feature("sctr1").selection().set(new int[]{1, 66});

    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").run("cpe1");
    model.mesh("mesh1").run("cpe2");
    model.mesh("mesh1").run("cpe3");
    model.mesh("mesh1").run("cpe4");
    model.mesh("mesh1").run("cpe5");
    model.mesh("mesh1").feature("ftri1").selection().set(new int[]{6, 7, 8, 10, 12, 14, 16});
    model.mesh("mesh1").run("ftri1");
    model.mesh("mesh1").feature("cpf1").selection("source").set(new int[]{6, 7, 14});
    model.mesh("mesh1").feature("cpf1").selection("destination").set(new int[]{59, 63, 64});
    model.mesh("mesh1").feature("cpf1").selection("source").remove(new int[]{7});
    model.mesh("mesh1").feature("cpf1").selection("source").remove(new int[]{14});
    model.mesh("mesh1").feature("cpf1").selection("destination").set(new int[]{59});
    model.mesh("mesh1").run("cpf1");
    model.mesh("mesh1").feature("cpf2").selection("source").set(new int[]{13});
    model.mesh("mesh1").feature("cpf2").selection("destination").remove(new int[]{18});
    model.mesh("mesh1").feature("cpf2").selection("destination").set(new int[]{63});
    model.mesh("mesh1").feature("cpf2").selection("source").remove(new int[]{13});
    model.mesh("mesh1").feature("cpf2").selection("destination").set(new int[]{});
    model.mesh("mesh1").feature("cpf1").selection("source").set(new int[]{6, 10, 14});
    model.mesh("mesh1").feature("cpf1").selection("destination").set(new int[]{59, 63, 64});
    model.mesh("mesh1").run("cpf1");
    model.mesh("mesh1").feature("cpf2").selection("source").set(new int[]{7});
    model.mesh("mesh1").feature("cpf2").selection("destination").set(new int[]{18});
    model.mesh("mesh1").run("cpf2");
    model.mesh("mesh1").run("cpf3");
    model.mesh("mesh1").feature("ftet1").selection().geom("geom1", 3);
    model.mesh("mesh1").feature("ftet1").selection().all();
    model.mesh("mesh1").feature("ftet1").selection().set(new int[]{2, 3, 4, 5, 6, 7, 8, 9, 10});
    model.mesh("mesh1").run("ftet1");
    model.mesh("mesh1").create("cpd1", "CopyDomain");
    model.mesh("mesh1").feature("cpd1").selection("source").set(new int[]{2, 3, 4, 5, 6, 7, 8, 9, 10});
    model.mesh("mesh1").feature("cpd1").selection("destination").set(new int[]{1, 11});
    model.mesh("mesh1").run("cpd1");
    model.mesh("mesh1").feature().remove("cpd1");
    model.mesh("mesh1").create("ftet2", "FreeTet");
    model.mesh("mesh1").feature("ftet2").create("size1", "Size");
    model.mesh("mesh1").feature("ftet2").feature("size1").set("custom", "on");
    model.mesh("mesh1").feature("ftet2").feature("size1").set("hmaxactive", "on");
    model.mesh("mesh1").feature("ftet2").feature("size1").set("hmax", "meshsize");
    model.mesh("mesh1").run("ftet2");
    model.mesh("mesh1").feature("ftet1").selection().remaining();
    model.mesh("mesh1").run("ftet1");
    model.mesh("mesh1").feature().remove("ftet2");

    model.study("std1").feature("param").setIndex("plistarr", "range(5.00[GHz],0.50[GHz],7.00[GHz])", 0);

    model.sol().remove("sol1");
    model.sol().remove("sol2");

    model.batch().remove("p1");

    model.sol().create("sol1");
    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "Default");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch().create("p1", "Parametric");
    model.batch("p1").study("std1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "none");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(5.00[GHz],0.50[GHz],7.00[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").set("pdistrib", "off");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");

    model.sol().create("sol2");
    model.sol("sol2").study("std1");
    model.sol("sol2").label("Parametric Solutions 1");

    model.batch("p1").feature("so1").set("psol", "sol2");

    model.result().create("pg1", "PlotGroup3D");
    model.result("pg1").label("Electric Field (emw)");
    model.result("pg1").set("data", "dset2");
    model.result("pg1").set("oldanalysistype", "noneavailable");
    model.result("pg1").set("frametype", "spatial");
    model.result("pg1").set("data", "dset2");
    model.result("pg1").feature().create("mslc1", "Multislice");
    model.result("pg1").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg1").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg1").run();

    model.mesh("mesh1").feature("ftet1").selection().geom("geom1", 3);
    model.mesh("mesh1").feature("ftet1").selection().set(new int[]{2, 3, 4, 5, 6, 7, 8, 9, 10});
    model.mesh("mesh1").run("ftet1");
    model.mesh("mesh1").create("cpd1", "CopyDomain");
    model.mesh("mesh1").feature("cpd1").selection("source").set(new int[]{2, 3, 4, 5, 6, 7, 8});
    model.mesh("mesh1").feature("cpd1").selection("destination").set(new int[]{});
    model.mesh("mesh1").feature("cpd1").selection("source").set(new int[]{2, 3, 4, 5, 6, 7, 8, 9, 10});
    model.mesh("mesh1").feature("cpd1").selection("destination").set(new int[]{1, 11});
    model.mesh("mesh1").run("cpd1");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(5.00[GHz],0.50[GHz],7.00[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").set("pdistrib", "off");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");

    model.result().create("pg2", "PlotGroup3D");
    model.result("pg2").label("Electric Field (emw) 1");
    model.result("pg2").set("oldanalysistype", "noneavailable");
    model.result("pg2").set("solvertype", "none");
    model.result("pg2").set("showlooplevel", new String[]{"off", "off", "off"});
    model.result("pg2").set("frametype", "spatial");
    model.result("pg2").set("data", "dset1");
    model.result("pg2").feature().create("mslc1", "Multislice");
    model.result("pg2").feature("mslc1").set("oldanalysistype", "noneavailable");
    model.result("pg2").feature("mslc1").set("solvertype", "none");
    model.result("pg2").feature("mslc1").set("data", "parent");

    model.batch("p1").run();

    model.result("pg1").run();
    model.result().create("pg3", "PlotGroup1D");
    model.result("pg3").run();
    model.result("pg3").create("glob1", "Global");
    model.result("pg3").feature("glob1").setIndex("expr", "T", 0);
    model.result("pg3").run();
    model.result("pg3").feature("glob1").set("data", "dset2");
    model.result("pg3").feature("glob1").set("xdatasolnumtype", "outer");
    model.result("pg3").run();

    model.mesh("mesh1").feature().remove("cpd1");
    model.mesh("mesh1").run("cpf1");
    model.mesh("mesh1").run("cpf2");
    model.mesh("mesh1").run("cpf3");
    model.mesh("mesh1").feature("cpf1").selection("destination").set(new int[]{1, 59, 63, 64});
    model.mesh("mesh1").current("cpf1");
    model.mesh("mesh1").feature("cpf1").selection("destination").set(new int[]{1, 63, 64});
    model.mesh("mesh1").current("cpf1");
    model.mesh("mesh1").feature("cpf1").selection("destination").remove(new int[]{1});
    model.mesh("mesh1").feature("cpf1").selection("destination").set(new int[]{59, 63, 64});
    model.mesh("mesh1").run("cpf1");
    model.mesh("mesh1").run("cpf2");
    model.mesh("mesh1").run("cpf3");
    model.mesh("mesh1").run("cpf3");
    model.mesh("mesh1").create("cpf4", "CopyFace");
    model.mesh("mesh1").feature("cpf4").selection("source").set(new int[]{7});
    model.mesh("mesh1").feature("cpf4").selection("destination").set(new int[]{2, 60});
    model.mesh("mesh1").run("cpf4");
    model.mesh("mesh1").feature("cpf4").selection("destination").set(new int[]{2, 5, 60});
    model.mesh("mesh1").run("cpf4");
    model.mesh("mesh1").feature("cpf4").selection("destination").set(new int[]{2, 5, 60, 65});
    model.mesh("mesh1").run("cpf4");
    model.mesh("mesh1").create("cpf5", "CopyFace");
    model.mesh("mesh1").feature("cpf5").selection("source").set(new int[]{8, 12, 16});
    model.mesh("mesh1").feature("cpf5").selection("destination").set(new int[]{3});
    model.mesh("mesh1").run("cpf5");
    model.mesh("mesh1").feature("cpf5").selection("destination").set(new int[]{3, 61});
    model.mesh("mesh1").run("cpf5");
    model.mesh("mesh1").feature("cpf5").selection("destination").set(new int[]{3, 4, 61});
    model.mesh("mesh1").run("cpf5");
    model.mesh("mesh1").feature("cpf5").selection("destination").set(new int[]{3, 4, 61, 62});
    model.mesh("mesh1").run("cpf5");
    model.mesh("mesh1").feature("ftet1").selection().remaining();
    model.mesh("mesh1").run("ftet1");
    model.mesh("mesh1").run("cpf3");

    return model;
  }

  public static Model run7(Model model) {
    model.mesh("mesh1").feature().remove("cpf4");
    model.mesh("mesh1").feature().remove("cpf5");
    model.mesh("mesh1").run("cpe1");
    model.mesh("mesh1").feature("cpe1").selection("destination").set(new int[]{1, 6, 24, 107, 121, 124});
    model.mesh("mesh1").run("cpe1");
    model.mesh("mesh1").feature("cpe2").selection("destination").set(new int[]{3, 5, 7, 13, 25, 109, 111, 119});
    model.mesh("mesh1").run("cpe2");
    model.mesh("mesh1").feature("cpe1").selection("destination").remove(new int[]{1});
    model.mesh("mesh1").feature("cpe1").selection("destination").remove(new int[]{6});
    model.mesh("mesh1").feature("cpe1").selection("destination").remove(new int[]{121});
    model.mesh("mesh1").feature("cpe1").selection("destination").remove(new int[]{124});
    model.mesh("mesh1").run("edg1");
    model.mesh("mesh1").run("cpe1");
    model.mesh("mesh1").run("cpe2");
    model.mesh("mesh1").run("cpe3");
    model.mesh("mesh1").run("cpe4");
    model.mesh("mesh1").run("cpe5");
    model.mesh("mesh1").feature("cpe2").selection("destination").remove(new int[]{7});
    model.mesh("mesh1").feature("cpe2").selection("destination").remove(new int[]{5});
    model.mesh("mesh1").feature("cpe2").selection("destination").remove(new int[]{3});
    model.mesh("mesh1").feature("cpe2").selection("destination").remove(new int[]{109});
    model.mesh("mesh1").feature("cpe2").selection("destination").remove(new int[]{111});
    model.mesh("mesh1").feature("cpe2").selection("destination").remove(new int[]{119});
    model.mesh("mesh1").run("cpe1");
    model.mesh("mesh1").run("cpe2");
    model.mesh("mesh1").run("cpe3");
    model.mesh("mesh1").run("cpe4");
    model.mesh("mesh1").run("cpe5");
    model.mesh("mesh1").run("ftri1");
    model.mesh("mesh1").run("cpf1");
    model.mesh("mesh1").run("cpf2");
    model.mesh("mesh1").run("cpf3");
    model.mesh("mesh1").create("cpe6", "CopyEdge");
    model.mesh("mesh1").feature("cpe6").selection("source").set(new int[]{11});
    model.mesh("mesh1").feature("cpe6").selection("destination").set(new int[]{3, 5, 7, 109, 111, 119});
    model.mesh("mesh1").run("cpe6");
    model.mesh("mesh1").create("ftri2", "FreeTri");
    model.mesh("mesh1").feature("ftri2").create("size1", "Size");
    model.mesh("mesh1").feature("ftri2").feature("size1").set("custom", "on");
    model.mesh("mesh1").feature("ftri2").feature("size1").set("hmaxactive", "on");
    model.mesh("mesh1").feature("ftri2").feature("size1").set("hmax", "meshsize");
    model.mesh("mesh1").feature("ftri2").selection().set(new int[]{2, 3, 60, 61});
    model.mesh("mesh1").run("ftri2");
    model.mesh("mesh1").create("cpf4", "CopyFace");
    model.mesh("mesh1").feature("cpf4").selection("source").set(new int[]{3});
    model.mesh("mesh1").feature("cpf4").selection("destination").set(new int[]{4});
    model.mesh("mesh1").run("cpf4");
    model.mesh("mesh1").create("cpf5", "CopyFace");
    model.mesh("mesh1").feature("cpf5").selection("source").set(new int[]{61});
    model.mesh("mesh1").feature("cpf5").selection("destination").set(new int[]{62});
    model.mesh("mesh1").run("cpf5");
    model.mesh("mesh1").create("cpf6", "CopyFace");
    model.mesh("mesh1").feature("cpf6").selection("source").set(new int[]{2});
    model.mesh("mesh1").feature("cpf6").selection("destination").set(new int[]{5});
    model.mesh("mesh1").run("cpf6");
    model.mesh("mesh1").create("cpf7", "CopyFace");
    model.mesh("mesh1").feature("cpf7").selection("source").set(new int[]{60});
    model.mesh("mesh1").feature("cpf7").selection("destination").set(new int[]{65});
    model.mesh("mesh1").run("cpf7");
    model.mesh("mesh1").run("ftet1");

    model.sol("sol1").study("std1");

    model.study("std1").feature("freq").set("notlistsolnum", 1);
    model.study("std1").feature("freq").set("notsolnum", "1");
    model.study("std1").feature("freq").set("listsolnum", 1);
    model.study("std1").feature("freq").set("solnum", "1");

    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
    model.sol("sol1").feature("s1").feature("p1").set("plot", "off");
    model.sol("sol1").feature("s1").feature("p1").set("plotgroup", "pg1");
    model.sol("sol1").feature("s1").feature("p1").set("probesel", "all");
    model.sol("sol1").feature("s1").feature("p1").set("probes", new String[]{});
    model.sol("sol1").feature("s1").feature("p1").set("control", "freq");
    model.sol("sol1").feature("s1").set("control", "freq");
    model.sol("sol1").feature("s1").feature("aDef").set("complexfun", true);
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "dDef");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(5.00[GHz],0.50[GHz],7.00[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").set("pdistrib", "off");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");
    model.batch("p1").run();

    model.result("pg1").run();
    model.result("pg3").run();

    model.label("SRR_tr_v50.mph");

    model.setGroupByType(false);
    model.component("comp1").setGroupByType(false);

    model.component("comp1").physics("emw").setGroupBySpaceDimension(false);

    model.result("pg3").run();

    model.study("std1").feature("param").setIndex("plistarr", "range(5.00[GHz],0.01[GHz],7.00[GHz])", 0);

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
    model.sol("sol1").feature("s1").set("stol", 0.01);
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
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
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").create("d1", "Direct");
    model.sol("sol1").feature("s1").feature("d1").set("linsolver", "mumps");
    model.sol("sol1").feature("s1").feature("d1").label("Suggested Direct Solver (emw)");
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "gmres");
    model.sol("sol1").feature("s1").feature("i1").set("prefuntype", "right");
    model.sol("sol1").feature("s1").feature("i1").set("itrestart", "300");
    model.sol("sol1").feature("s1").feature("i1").label("Suggested Iterative Solver (emw)");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").set("iter", "1");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("va1", "Vanka");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankavars", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankasolv", new String[]{"stored"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankarelax", 0.95);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("relax", 0.5);
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "d1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(5.00[GHz],0.01[GHz],7.00[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");
    model.batch("p1").run("compute");

    model.result("pg1").run();
    model.result("pg3").run();
    model.result("pg3").run();
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("expr", "rea", 0);
    model.result("pg3").feature("glob1").setIndex("expr", "T", 0);
    model.result("pg1").run();
    model.result().numerical().create("gev1", "EvalGlobal");
    model.result().numerical("gev1").setIndex("expr", "real(t)", 0);
    model.result().numerical("gev1").setIndex("expr", "real(tf)", 0);
    model.result().numerical("gev1").setIndex("expr", "imag(tf)", 1);
    model.result().numerical("gev1").setIndex("expr", "real(rf)", 2);
    model.result().numerical("gev1").setIndex("expr", "imag(rf)", 3);
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").set("data", "dset2");
    model.result().table().remove("tbl1");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().numerical("gev1").setIndex("looplevelinput", "first", 0);
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().table().remove("tbl1");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().table("tbl1")
         .save("C:\\Users\\admin\\Documents\\jensen\\projects\\TOBnotes\\master\\effectivemedium\\fieldavg\\negindex\\SRR_tr_unit1_v62.dat");

    model.param().set("aa", "1-i*sigmaCu/omega0/eps0");
    model.param().rename("epsCu", "epsCu0");
    model.param().rename("aa", "epsCu");

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
    model.sol("sol1").feature("s1").set("stol", 0.01);
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
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
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").create("d1", "Direct");
    model.sol("sol1").feature("s1").feature("d1").set("linsolver", "mumps");
    model.sol("sol1").feature("s1").feature("d1").label("Suggested Direct Solver (emw)");
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "gmres");
    model.sol("sol1").feature("s1").feature("i1").set("prefuntype", "right");
    model.sol("sol1").feature("s1").feature("i1").set("itrestart", "300");
    model.sol("sol1").feature("s1").feature("i1").label("Suggested Iterative Solver (emw)");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").set("iter", "1");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("va1", "Vanka");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankavars", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankasolv", new String[]{"stored"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankarelax", 0.95);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("relax", 0.5);
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "d1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(5.00[GHz],0.01[GHz],7.00[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");
    model.batch("p1").run("compute");

    model.result("pg1").run();
    model.result().table().remove("tbl1");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().table("tbl1")
         .save("C:\\Users\\admin\\Documents\\jensen\\projects\\TOBnotes\\master\\effectivemedium\\fieldavg\\negindex\\SRR_tr_unit1_v62.dat");
    model.result().table().remove("tbl1");
    model.result().table().create("tbl1", "Table");
    model.result().table("tbl1").comments("Global Evaluation 1");
    model.result().numerical("gev1").set("table", "tbl1");
    model.result().numerical("gev1").setResult();
    model.result().table("tbl1")
         .save("C:\\Users\\admin\\Documents\\jensen\\projects\\TOBnotes\\master\\effectivemedium\\fieldavg\\negindex\\SRR_tr_unit1_v62.dat");

    model.study("std1").feature("param").setIndex("plistarr", "range(5.00[GHz],0.5[GHz],7.00[GHz])", 0);

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
    model.sol("sol1").feature("s1").set("stol", 0.01);
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
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
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").create("d1", "Direct");
    model.sol("sol1").feature("s1").feature("d1").set("linsolver", "mumps");
    model.sol("sol1").feature("s1").feature("d1").label("Suggested Direct Solver (emw)");
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "gmres");
    model.sol("sol1").feature("s1").feature("i1").set("prefuntype", "right");
    model.sol("sol1").feature("s1").feature("i1").set("itrestart", "300");
    model.sol("sol1").feature("s1").feature("i1").label("Suggested Iterative Solver (emw)");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").set("iter", "1");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("va1", "Vanka");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankavars", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankasolv", new String[]{"stored"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankarelax", 0.95);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("relax", 0.5);
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "d1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(5.00[GHz],0.5[GHz],7.00[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");
    model.batch("p1").run("compute");

    model.result("pg1").run();

    model.label("SRR_tr_unit1_v62.mph");

    model.result("pg1").run();

    model.component("comp1").view("view1").set("transparency", true);
    model.component("comp1").view("view1").set("renderwireframe", true);
    model.component("comp1").view("view1").set("transparency", false);
    model.component("comp1").view("view1").set("showaxisorientation", true);

    model.label("SRR_tr_unit1_v62.mph");

    model.result("pg3").run();

    model.study("std1").feature("param").setIndex("plistarr", "range(5.00[GHz],0.25[GHz],7.00[GHz])", 0);

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
    model.sol("sol1").feature("s1").set("stol", 0.01);
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
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
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").create("d1", "Direct");
    model.sol("sol1").feature("s1").feature("d1").set("linsolver", "mumps");
    model.sol("sol1").feature("s1").feature("d1").label("Suggested Direct Solver (emw)");
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "gmres");
    model.sol("sol1").feature("s1").feature("i1").set("prefuntype", "right");
    model.sol("sol1").feature("s1").feature("i1").set("itrestart", "300");
    model.sol("sol1").feature("s1").feature("i1").label("Suggested Iterative Solver (emw)");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").set("iter", "1");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("va1", "Vanka");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankavars", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankasolv", new String[]{"stored"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankarelax", 0.95);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("relax", 0.5);
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "d1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(5.00[GHz],0.25[GHz],7.00[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");
    model.batch("p1").run("compute");

    model.result("pg1").run();
    model.result("pg3").run();
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("expr", "R", 1);
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("expr", "T+R", 2);
    model.result("pg3").run();

    model.component("comp1").physics("emw").create("port1", "Port", 2);
    model.component("comp1").physics("emw").feature("port1").selection().set(1);
    model.component("comp1").physics("emw").feature("sctr1").selection().set(1, 66);
    model.component("comp1").physics("emw").feature("port1").active(false);
    model.component("comp1").physics("emw").prop("BackgroundField").set("SolveFor", "fullField");
    model.component("comp1").physics("emw").feature().duplicate("sctr2", "sctr1");
    model.component("comp1").physics("emw").feature().move("sctr2", 7);
    model.component("comp1").physics("emw").feature("sctr1").selection().set(1);
    model.component("comp1").physics("emw").feature("sctr2").selection().set(66);
    model.component("comp1").physics("emw").feature("sctr1").set("IncidentField", "EField");
    model.component("comp1").physics("emw").feature("sctr1").set("E0i", new String[]{"0", "0", "Ezin"});

    model.sol("sol1").study("std1");

    return model;
  }

  public static Model run8(Model model) {
    model.sol("sol1").feature().remove("s1");
    model.sol("sol1").feature().remove("v1");
    model.sol("sol1").feature().remove("st1");
    model.sol("sol1").create("st1", "StudyStep");
    model.sol("sol1").feature("st1").set("study", "std1");
    model.sol("sol1").feature("st1").set("studystep", "freq");
    model.sol("sol1").create("v1", "Variables");
    model.sol("sol1").feature("v1").set("control", "freq");
    model.sol("sol1").create("s1", "Stationary");
    model.sol("sol1").feature("s1").set("stol", 0.01);
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
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
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").create("d1", "Direct");
    model.sol("sol1").feature("s1").feature("d1").set("linsolver", "mumps");
    model.sol("sol1").feature("s1").feature("d1").label("Suggested Direct Solver (emw)");
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "gmres");
    model.sol("sol1").feature("s1").feature("i1").set("prefuntype", "right");
    model.sol("sol1").feature("s1").feature("i1").set("itrestart", "300");
    model.sol("sol1").feature("s1").feature("i1").label("Suggested Iterative Solver (emw)");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").set("iter", "1");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("va1", "Vanka");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankavars", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankasolv", new String[]{"stored"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankarelax", 0.95);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("relax", 0.5);
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "d1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(5.00[GHz],0.25[GHz],7.00[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");
    model.batch("p1").run("compute");

    model.result("pg1").run();
    model.result("pg3").run();

    model.component("comp1").physics("emw").feature("port1").active(true);
    model.component("comp1").physics("emw").feature("port1").set("E0", new String[]{"0", "0", "Ezin"});
    model.component("comp1").physics("emw").feature().duplicate("port2", "port1");
    model.component("comp1").physics("emw").feature("port2").selection().set(66);
    model.component("comp1").physics("emw").feature("port2").set("PortExcitation", "off");
    model.component("comp1").physics("emw").feature("port2").set("E0", new int[]{0, 0, 1});
    model.component("comp1").physics("emw").feature("port1").set("E0", new int[]{0, 0, 1});
    model.component("comp1").physics("emw").feature("sctr1").active(false);
    model.component("comp1").physics("emw").feature("sctr2").active(false);

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
    model.sol("sol1").feature("s1").set("stol", 0.01);
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
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
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").create("d1", "Direct");
    model.sol("sol1").feature("s1").feature("d1").set("linsolver", "mumps");
    model.sol("sol1").feature("s1").feature("d1").label("Suggested Direct Solver (emw)");
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "gmres");
    model.sol("sol1").feature("s1").feature("i1").set("prefuntype", "right");
    model.sol("sol1").feature("s1").feature("i1").set("itrestart", "300");
    model.sol("sol1").feature("s1").feature("i1").label("Suggested Iterative Solver (emw)");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").set("iter", "1");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("va1", "Vanka");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankavars", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankasolv", new String[]{"stored"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankarelax", 0.95);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("relax", 0.5);
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "d1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(5.00[GHz],0.25[GHz],7.00[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");
    model.batch("p1").run("compute");

    model.result("pg1").run();
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("expr", "abs(emw.S21)^2", 0);
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("expr", "abs(emw.S11)^2", 1);
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("expr", "abs(emw.S11)^2", 2);
    model.result("pg3").feature("glob1").setIndex("expr", "abs(emw.S21)^2+abs(emw.S11)^2", 2);
    model.result("pg3").run();
    model.result("pg3").run();
    model.result("pg3").run();
    model.result("pg3").run();
    model.result("pg3").run();
    model.result("pg1").run();
    model.result("pg3").run();
    model.result("pg3").run();
    model.result("pg3").feature("glob1").remove("unit", 1);
    model.result("pg3").feature("glob1").remove("descr", 1);
    model.result("pg3").feature("glob1").remove("expr", new int[]{1});
    model.result("pg3").feature("glob1").remove("unit", 1);
    model.result("pg3").feature("glob1").remove("descr", 1);
    model.result("pg3").feature("glob1").remove("expr", new int[]{1});
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("expr", "abs(emw.S11)^2", 0);
    model.result("pg3").run();

    model.component("comp1").physics("emw").feature("port2").set("PortType", "Periodic");
    model.component("comp1").physics("emw").feature("port2").set("Eampl", new int[]{0, 0, 1});
    model.component("comp1").physics("emw").feature("port1").set("PortType", "Periodic");
    model.component("comp1").physics("emw").feature("port1").set("Eampl", new int[]{0, 0, 1});

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
    model.sol("sol1").feature("s1").set("stol", 0.01);
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
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
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").create("d1", "Direct");
    model.sol("sol1").feature("s1").feature("d1").set("linsolver", "mumps");
    model.sol("sol1").feature("s1").feature("d1").label("Suggested Direct Solver (emw)");
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "gmres");
    model.sol("sol1").feature("s1").feature("i1").set("prefuntype", "right");
    model.sol("sol1").feature("s1").feature("i1").set("itrestart", "300");
    model.sol("sol1").feature("s1").feature("i1").label("Suggested Iterative Solver (emw)");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").set("iter", "1");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("va1", "Vanka");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankavars", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankasolv", new String[]{"stored"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankarelax", 0.95);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("relax", 0.5);
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "d1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(5.00[GHz],0.25[GHz],7.00[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");
    model.batch("p1").run("compute");

    model.result("pg1").run();
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("expr", "abs(emw.S11)^2", 1);
    model.result("pg3").feature("glob1").setIndex("expr", "abs(emw.S21)^2", 0);
    model.result("pg3").run();
    model.result("pg3").feature("glob1").setIndex("expr", "abs(emw.S21)^2", 2);
    model.result("pg3").feature("glob1").setIndex("expr", "abs(emw.S21)^2+abs(emw.S11)^2", 2);
    model.result("pg3").run();

    model.component("comp1").physics("emw").feature().remove("sctr1");
    model.component("comp1").physics("emw").feature("sctr2").active(true);
    model.component("comp1").physics("emw").feature().remove("sctr2");

    model.result("pg1").run();
    model.result("pg2").run();
    model.result("pg3").run();

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
    model.sol("sol1").feature("s1").set("stol", 0.01);
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
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
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").create("d1", "Direct");
    model.sol("sol1").feature("s1").feature("d1").set("linsolver", "mumps");
    model.sol("sol1").feature("s1").feature("d1").label("Suggested Direct Solver (emw)");
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "gmres");
    model.sol("sol1").feature("s1").feature("i1").set("prefuntype", "right");
    model.sol("sol1").feature("s1").feature("i1").set("itrestart", "300");
    model.sol("sol1").feature("s1").feature("i1").label("Suggested Iterative Solver (emw)");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").set("iter", "1");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("va1", "Vanka");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankavars", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankasolv", new String[]{"stored"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankarelax", 0.95);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("relax", 0.5);
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "d1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(5.00[GHz],0.25[GHz],7.00[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");
    model.batch("p1").run("compute");

    model.result("pg1").run();
    model.result("pg3").run();

    model.variable().remove("var6");
    model.variable().remove("var7");

    model.component("comp1").cpl().remove("intop1");
    model.component("comp1").cpl().remove("intop2");

    model.component("comp1").view("view1").set("transparency", true);

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
    model.sol("sol1").feature("s1").set("stol", 0.01);
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
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
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").create("d1", "Direct");
    model.sol("sol1").feature("s1").feature("d1").set("linsolver", "mumps");
    model.sol("sol1").feature("s1").feature("d1").label("Suggested Direct Solver (emw)");
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "gmres");
    model.sol("sol1").feature("s1").feature("i1").set("prefuntype", "right");
    model.sol("sol1").feature("s1").feature("i1").set("itrestart", "300");
    model.sol("sol1").feature("s1").feature("i1").label("Suggested Iterative Solver (emw)");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").set("iter", "1");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("va1", "Vanka");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankavars", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankasolv", new String[]{"stored"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankarelax", 0.95);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("relax", 0.5);
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "d1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(5.00[GHz],0.25[GHz],7.00[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");
    model.batch("p1").run("compute");

    model.result("pg1").run();
    model.result("pg3").run();

    model.label("SRR_tr_unit1_v62.mph");

    model.result("pg3").run();
    model.result("pg1").run();
    model.result("pg2").run();
    model.result("pg3").run();

    model.label("SRR_tr_unit1_v62.mph");

    model.result("pg3").run();

    model.label("SRR_tr_unit1_v62.mph");

    model.result("pg3").run();

    model.component("comp1").mesh("mesh1").stat().setQualityMeasure("skewness");
    model.component("comp1").mesh("mesh1").stat().selection().allGeom();
    model.component("comp1").mesh("mesh1").run();

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
    model.sol("sol1").feature("s1").set("stol", 0.01);
    model.sol("sol1").feature("s1").create("p1", "Parametric");
    model.sol("sol1").feature("s1").feature().remove("pDef");
    model.sol("sol1").feature("s1").feature("p1").set("pname", new String[]{"freq"});
    model.sol("sol1").feature("s1").feature("p1").set("plistarr", new String[]{"freq0"});
    model.sol("sol1").feature("s1").feature("p1").set("punit", new String[]{"Hz"});
    model.sol("sol1").feature("s1").feature("p1").set("pcontinuationmode", "no");
    model.sol("sol1").feature("s1").feature("p1").set("preusesol", "auto");
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
    model.sol("sol1").feature("s1").create("fc1", "FullyCoupled");
    model.sol("sol1").feature("s1").create("d1", "Direct");
    model.sol("sol1").feature("s1").feature("d1").set("linsolver", "mumps");
    model.sol("sol1").feature("s1").feature("d1").label("Suggested Direct Solver (emw)");
    model.sol("sol1").feature("s1").create("i1", "Iterative");
    model.sol("sol1").feature("s1").feature("i1").set("linsolver", "gmres");
    model.sol("sol1").feature("s1").feature("i1").set("prefuntype", "right");
    model.sol("sol1").feature("s1").feature("i1").set("itrestart", "300");
    model.sol("sol1").feature("s1").feature("i1").label("Suggested Iterative Solver (emw)");
    model.sol("sol1").feature("s1").feature("i1").create("mg1", "Multigrid");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").set("iter", "1");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").create("va1", "Vanka");

    return model;
  }

  public static Model run9(Model model) {
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankavars", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankasolv", new String[]{"stored"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("pr").feature("va1")
         .set("vankarelax", 0.95);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").create("sv1", "SORVector");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("prefun", "soruvec");
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1")
         .set("sorvecdof", new String[]{"comp1_E"});
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("iter", 1);
    model.sol("sol1").feature("s1").feature("i1").feature("mg1").feature("po").feature("sv1").set("relax", 0.5);
    model.sol("sol1").feature("s1").feature("fc1").set("linsolver", "d1");
    model.sol("sol1").feature("s1").feature().remove("fcDef");
    model.sol("sol1").attach("std1");

    model.batch("p1").feature().remove("so1");
    model.batch("p1").create("so1", "Solutionseq");
    model.batch("p1").feature("so1").set("seq", "sol1");
    model.batch("p1").feature("so1").set("store", "on");
    model.batch("p1").feature("so1").set("clear", "on");
    model.batch("p1").feature("so1").set("psol", "sol2");
    model.batch("p1").set("pname", new String[]{"freq0"});
    model.batch("p1").set("plistarr", new String[]{"range(5.00[GHz],0.25[GHz],7.00[GHz])"});
    model.batch("p1").set("sweeptype", "sparse");
    model.batch("p1").set("probesel", "all");
    model.batch("p1").set("probes", new String[]{});
    model.batch("p1").set("plot", "off");
    model.batch("p1").set("err", "on");
    model.batch("p1").attach("std1");
    model.batch("p1").set("control", "param");
    model.batch("p1").run("compute");

    model.result("pg1").run();
    model.result("pg3").run();

    return model;
  }

  public static void main(String[] args) {
    Model model = run();
    model = run2(model);
    model = run3(model);
    model = run4(model);
    model = run5(model);
    model = run6(model);
    model = run7(model);
    model = run8(model);
    run9(model);
  }

}
