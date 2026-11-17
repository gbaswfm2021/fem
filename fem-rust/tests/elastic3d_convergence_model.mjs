// Disposable FEM case derived from the elastic3d implementation instruction.
// Writes the model to stdout for the normal --server Study path.
const size = Number(process.argv[2] ?? '0.0025');
const start = Number(process.argv[3] ?? '550');
const end = Number(process.argv[4] ?? '1100');
const count = Number(process.argv[5] ?? '12');
if (!(size > 0 && start > 0 && end >= start && Number.isInteger(count) && count >= 2)) throw new Error('Invalid convergence case arguments.');
const q=(expression,unit)=>({expression:String(expression),unit});
const model={
  application:'fem',metadata:{id:'elastic_convergence',name:'Elastic convergence case'},modelFormat:'fem-model/2',
  scenario:{dimension:3,physics:'elasticity',profileId:'elastic-3d'},'x-fullmeta-schema':'../../fem.schema.json',
  model:{units:'m',parameters:[{id:'scan',name:'Frequency',symbol:'fscan',expression:String(start),unit:'Hz'}],
    variables:[],pointwiseVariables:[],integrationOperators:[],
    geometry:{tolerance:q('1e-9','m'),features:[{id:'plate',name:'Plate',type:'box',position:[q(0,'m'),q(0,'m'),q(0,'m')],size:[q(0.2,'m'),q(0.1,'m'),q(0.004,'m')]}]},
    selections:[{id:'domain',name:'Plate',entity:'domain',addresses:[{membership:{plate:'inside'}}]},
      {id:'side',name:'Side',entity:'boundary',addresses:[{membership:{plate:'boundary'}}],coordinateExpression:'x'}],
    materials:[{id:'material',name:'Elastic material',selectionId:'domain',density:q(2700,'kg/m^3'),youngModulus:q(7e10,'Pa'),poissonRatio:'0.33',densityLossFactor:'0.01'}],
    mesh:{elementFamily:'tetrahedron',gmsh:{algorithm3D:1,elementOrder:1,optimize:true,optimizeNetgen:true,optimizeThreshold:0.25,smoothing:10,threads:1},
      minSize:q(size/2,'m'),maxSize:q(size,'m'),meshSizeControls:[],periodicPairs:[]},
    physics:{id:'physics',name:'Elasticity',type:'elasticity',formulation:'displacement',domainSelectionIds:['domain'],frequency:q('fscan','Hz'),
      load:{id:'load',name:'Traction',type:'neumann',selectionId:'side',value:q(0,'Pa'),fieldValues:[q(0,'Pa'),q(0,'Pa'),q(2500,'Pa')]}},
    study:[{id:'probe',name:'Convergence probe',type:'frequencyDomain',physicsId:'physics',discretization:{elementOrder:1,fieldFormulation:'displacement'},
      solver:{backend:'faer',ordering:'default',threads:1,constitutiveQuadrature:'standard'},
      parameterSweep:{id:'axis',name:'Frequency',type:'parameterSweep',parameters:[{parameter:'fscan',values:{type:'linear',start:q(start,'Hz'),end:q(end,'Hz'),count}}]},
      output:{csvFilename:'convergence.csv',fieldFiles:false}}],results:[]}
};
process.stdout.write(JSON.stringify(model));
