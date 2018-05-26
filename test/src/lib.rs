extern crate capnp;
extern crate capnp_rpc;
extern crate futures;
extern crate mercury_connect;
extern crate mercury_home_protocol;
extern crate mercury_home_node;
extern crate tokio_core;
extern crate tokio_io;
extern crate multiaddr;
extern crate multihash;
extern crate tokio_stdin_stdout;

pub mod dummy;

#[cfg(test)]
mod test{
    use super::*;
    use ::dummy::*;

    use std::net::ToSocketAddrs;
    use std::rc::Rc;
    use std::cell::RefCell;

    use multiaddr::{ToMultiaddr};

    use futures::future;
    use futures::{Future, Stream, Sink};
    use futures::sync::mpsc;

    use tokio_core::net::{TcpListener, TcpStream};
    use tokio_core::reactor;

    use mercury_home_protocol::{*, crypto::*};
    use mercury_connect::*;
    use ::dummy::{ MyDummyHome, make_home_profile, ProfileStore, };
    use mercury_connect::protocol_capnp::HomeClientCapnProto;
    use mercury_home_node::protocol_capnp::HomeDispatcherCapnProto;

    use mercury_connect::ProfileGateway;
    use mercury_connect::ProfileGatewayImpl;

    #[test]
    fn test_events()
    {
        let mut reactor = reactor::Core::new().unwrap();

        let homeaddr = "127.0.0.1:9876";
        let addr = homeaddr.clone().to_socket_addrs().unwrap().next().expect("Failed to parse address");

        let homemultiaddr = "/ip4/127.0.0.1/udp/9876".to_multiaddr().unwrap();
        let (homeprof, _homesigno) = crypto::generate_profile(ProfileFacet::Home(HomeFacet{addrs: vec![homemultiaddr.clone()], data: vec![]}));

        let mut dht = ProfileStore::new();
        dht.insert(homeprof.id.clone(), homeprof.clone());
        let home_storage = Rc::new( RefCell::new(dht) );

        let handle1 = reactor.handle();
        let server_socket = TcpListener::bind( &addr, &reactor.handle() ).expect("Failed to bind socket");
        let server_fut = server_socket.incoming().for_each( move |(socket, _addr)|
        {
            println!("Accepted client connection, serving requests");
            //let mut home = Rc::new( RefCell::new( MyDummyHome::new( homeprof.clone() , home_storage ) ) );
            let store_clone = Rc::clone(&home_storage);
            let home = Box::new( MyDummyHome::new( homeprof.clone() , store_clone ) );
            HomeDispatcherCapnProto::dispatch_tcp( home, socket, handle1.clone() );
            Ok( () )
        } ).map_err( |_e| ErrorToBeSpecified::TODO(String::from("test_events fails at connect ")));

        let handle2 = reactor.handle();
        let client_fut = TcpStream::connect( &addr, &reactor.handle() )
            .map_err( |_e| ErrorToBeSpecified::TODO(String::from("test_events fails at connect ")))
            .and_then( |tcp_stream|
            {
                let (private_key, public_key) = crypto::generate_keypair();
                let signer = Rc::new(Ed25519Signer::new(&private_key, &public_key).unwrap());
                let my_profile = signer.prof_id().clone();
                let home_profile = make_home_profile("localhost:9876", signer.pub_key());
                let home_ctx = Box::new(HomeContext::new(signer, &home_profile));
                let client = HomeClientCapnProto::new_tcp( tcp_stream, home_ctx, handle2 );
                client.login(my_profile) // TODO maybe we should require only a reference in login()
            } )
            .map( |session|
            {
                session.events() //.for_each( |event| () )
            } );

    //    let futs = server_fut.select(client_fut);
    //    let both_fut = select_ok( futs.iter() ); // **i as &Future<Item=(),Error=()> ) );
    //    let result = reactor.run(both_fut);
        let result = reactor.run(Future::join(server_fut,client_fut));
        assert!(result.is_ok());
    }

    #[test]
    fn test_register(){

        let mut setup = dummy::TestSetup::setup();

        let mut registered_ownprofile = setup.userownprofile.clone();
        let relation_proof = RelationProof::new(
            "home", 
            &registered_ownprofile.profile.id, 
            &Signature(registered_ownprofile.profile.pub_key.0.clone()), 
            &setup.homeprofile.id, 
            &Signature(setup.homeprofile.pub_key.0.clone())
        );
        
        match registered_ownprofile.profile.facets[0]{
            ProfileFacet::Persona(ref mut facet)=>{
                facet.homes.push(relation_proof);
            },
            _=>{
                panic!("test_register failed cause Deusz fucked up");
            }
        }

        let ownprofile = setup.profilegate.register(
                setup.homeprofileid,
                setup.userownprofile,
                None
        );

        let res = setup.reactor.run(ownprofile).unwrap();
   
        assert_eq!(res, registered_ownprofile);  
    }

    #[test]
    fn test_unregister(){
        let mut setup = dummy::TestSetup::setup();

        //homeless_profile might be unneeded because unregistering does not give back a profile rid of home X    
        let _homeless_profile = setup.userownprofile.clone();
        let homeid = setup.homeprofileid.clone();
        let userid = setup.userid.clone();
        let registered = setup.profilegate.register(
                setup.homeprofileid.clone(),
                setup.userownprofile.clone(),
                None
        );
        let reg = setup.reactor.run(registered).unwrap();
        println!("{:?}", reg);
        //assert!(reg.is_ok());
        //see test_register() to see if registering works as intended
        let unreg = setup.profilegate.unregister(
            homeid,
            userid,
            None
        );
        let res = setup.reactor.run(unreg);
        assert!(res.is_err()); 
        //TODO needs HomeSession unregister implementation    
        //assert_eq!(res, homeless_profile);
    }

    #[test]
    fn test_login(){

        let mut setup = dummy::TestSetup::setup();

        let home_session = setup.profilegate.login();

        let res = setup.reactor.run(home_session); 
        assert!(res.is_ok());     
    }

    #[test]
    fn test_ping(){
        //TODO ping function only present for testing phase, incorporate into test_login?
        let mut setup = dummy::TestSetup::setup();

        let response = setup.profilegate.login()
        .and_then(|home_session|{
            home_session.ping( "test_ping" )
        });

        let res = setup.reactor.run(response);
        assert!(res.is_ok());      
    }

    #[test]
    fn test_claim(){
        //profile registering is required
        let mut setup = dummy::TestSetup::setup();

        let home_session = setup.profilegate.claim(
                setup.homeprofileid,
                setup.userid,
        );

        let res = setup.reactor.run(home_session).unwrap();
        //TODO needs home.claim implementation
        println!("Claimed : {:?} ||| Stored : {:?}", res, setup.userownprofile);        
        assert_eq!(res, setup.userownprofile);      
    }
    
    #[test]
    fn test_update(){

        let mut setup = dummy::TestSetup::setup();

        let homemultiaddr = "/ip4/127.0.0.1/udp/9876".to_multiaddr().unwrap();
        let (otherhome, _other_home_signer) = crypto::generate_profile(ProfileFacet::Home(HomeFacet{addrs: vec![homemultiaddr.clone()], data: vec![]}));

        setup.home.borrow_mut().insert(otherhome.id.clone(), otherhome.clone());
        let home_session = setup.profilegate.update(
            otherhome.id,
            &setup.userownprofile,
        );
        //TODO needs homesession.update implementation
        //session updates profile stored on home(?)
        let res = setup.reactor.run(home_session);
        assert!(res.is_ok());      
    }

    #[test]
    fn test_call(){

        let mut setup = dummy::TestSetup::setup();

        let call_messages = setup.profilegate.call(
            dummy::dummy_relation("test_relation"),
            ApplicationId( String::from( "Undertale" ) ), 
            AppMessageFrame( Vec::from( "Megalovania" ) ),
            None
        );
        //TODO needs home.call implementation...
        let res = setup.reactor.run(call_messages); 
        assert!(res.is_ok());     
    }

    #[test]
    fn test_pair_req(){
        //TODO could be tested by sending pair request and asserting the events half_proof that the peer receives to what is should be
        //let signo = Rc::new( dummy::Signo::new( "TestKey" ) );
        let mut setup = dummy::TestSetup::setup();

        let zero = setup.profilegate.pair_request( "test_relation", "test_url" );

        let res = setup.reactor.run(zero);   
        assert!(res.is_ok());
    }

    #[test]
    fn test_pair_res(){
        //TODO could be tested by sending pair response and asserting the events relation_proof that the peer receives to what is should be
        let mut setup = dummy::TestSetup::setup();
        let zero = setup.profilegate.pair_response(
                dummy::dummy_relation("test_relation"));

        let res = setup.reactor.run(zero);
        assert!(res.is_ok());      
    }

    #[test]
    fn test_relations(){
        //TODO test by storing relations and asserting the return value of relations to those that were stored
        let mut setup = dummy::TestSetup::setup();

        let zero = setup.profilegate.relations( &setup.userid );

        //let relations = None;
        let res = setup.reactor.run(zero);
        assert!(res.is_err());
    }

    #[test]
    fn and_then_story(){
        //print!("{}[2J", 27 as char);
        //println!( "***Setting up reactor and address variable" );
        let mut reactor = tokio_core::reactor::Core::new().unwrap();
        //let handle = reactor.handle();

        let homemultiaddr = "/ip4/127.0.0.1/udp/9876".to_multiaddr().unwrap();
        let (homeprof, homesigno) = crypto::generate_profile(ProfileFacet::Home(HomeFacet{addrs: vec![homemultiaddr.clone()], data: vec![]}));

        let homemultiaddr = "/ip4/127.0.0.1/udp/9877".to_multiaddr().unwrap();
        let (other_homeprof, other_homesigno) = crypto::generate_profile(ProfileFacet::Home(HomeFacet{addrs: vec![homemultiaddr.clone()], data: vec![]}));

        let mut dht = ProfileStore::new();
        dht.insert(homeprof.id.clone(), homeprof.clone());
        dht.insert(other_homeprof.id.clone(), other_homeprof.clone());

        let home_storage = Rc::new( RefCell::new(dht) );
        let ownhomestore = Rc::clone(&home_storage);
        let home = Rc::new( RefCell::new( MyDummyHome::new( homeprof.clone() , Rc::clone(&home_storage) ) ) );

        let (profile, signo) = crypto::generate_profile(ProfileFacet::Persona(PersonaFacet{homes: vec![], data: vec![]}));
        let signo = Rc::new(signo);

        let (_other_profile, other_signo) = crypto::generate_profile(ProfileFacet::Persona(PersonaFacet{homes: vec![], data: vec![]}));
        let other_signo = Rc::new(other_signo);

        let own_gateway = ProfileGatewayImpl::new(
            signo,
            ownhomestore,
            Rc::new( dummy::DummyConnector::new_with_home( home ) ),
        );
        
        let (reg_sender, reg_receiver) : (mpsc::Sender<String>, mpsc::Receiver<String>) = mpsc::channel(1);
        let (request_sender, request_receiver) : (mpsc::Sender<String>, mpsc::Receiver<String>) = mpsc::channel(1);

        let sess = own_gateway.register(
                homesigno.prof_id().to_owned(),
                dummy::create_ownprofile( profile.clone() ),
                None
        )
        .map_err(|(_p, e)|e)
        .join( reg_receiver.take(1).collect().map_err(|_e|ErrorToBeSpecified::TODO(String::from("cannot join on receive"))) )                
        .and_then(|_reg_string|{
            println!("user_one_requests");
            let f = other_signo.prof_id().0.clone();
            let problem = unsafe{String::from_utf8_unchecked(f)};
            own_gateway.pair_request( "relation_dummy_type", &problem )
        })
        .and_then(| _ |{
            request_sender.send(String::from("Other user registered")).map_err(|_e|ErrorToBeSpecified::TODO(String::from("cannot join on receive")))
        })
        .and_then(|_own_profile|{
            println!( "user_one_login" );
            own_gateway.login()
        })
        .and_then(|session|{
            println!("user_one_events");
            session.events().take(1).collect()
            .map_err(|_|ErrorToBeSpecified::TODO(String::from("pairing responded but something went wrong")))
        })
        .and_then(|pair_resp|{
            let resp_event = &pair_resp[0];
            println!("user_one_gets_response");
            match resp_event{
                &Ok(ProfileEvent::PairingResponse(ref relation_proof))=>{
                    println!("{:?}", relation_proof);
                    future::ok(relation_proof.clone())
                },
                _=>panic!("ProfileEvent assert fail")
            }
        })
        .and_then(|relation_proof|{
            let (msg_sender, msg_receiver) : (mpsc::Sender<Result<AppMessageFrame, String>>, mpsc::Receiver<Result<AppMessageFrame, String>>) = mpsc::channel(1);

            println!( "***call(RelationWithCallee, InWhatApp, InitMessage) -> CallMessages" );
            let relation = Relation::new(&profile,&relation_proof);
            own_gateway.call(
                relation,
                ApplicationId( String::from( "SampleApp" ) ), 
                AppMessageFrame( Vec::from( "whatever" ) ),
                Some(msg_sender)
            );
            println!("user_one_line_end");
            future::ok( msg_receiver )
        })
        .and_then(|rec|{
            rec.take(1).collect().map_err(|_e|ErrorToBeSpecified::TODO(String::from("message answer error")))
        })
        .and_then(|msg|{
            println!("{:?}", msg);
            future::ok(())
        });

        let other_home = Rc::new( RefCell::new( MyDummyHome::new( homeprof.clone() , Rc::clone(&home_storage) ) ) );
        let home_storage_other = Rc::clone(&home_storage);

        let other_profile = make_own_persona_profile(other_signo.pub_key() );
        let other_gateway = ProfileGatewayImpl::new(
            other_signo.clone(), 
            home_storage_other,
            Rc::new( dummy::DummyConnector::new_with_home( other_home ) ),
        );

        // let mut othersession : Box<HomeSession>;
        let other_reg = other_gateway.register(
            other_homesigno.prof_id().to_owned(),
            dummy::create_ownprofile( other_profile.clone() ),
            None
        )
        .map_err(|(_p,e)|e)
        .and_then(| _ |{
            reg_sender.send(String::from("Other user registered")).map_err(|_e|ErrorToBeSpecified::TODO(String::from("cannot join on receive")))
        })
        .join( request_receiver.take(1).collect().map_err(|_e|ErrorToBeSpecified::TODO(String::from("cannot join on receive"))) )
        .and_then(| _ |{
            println!("user_two_login");
            other_gateway.login()
        })
        .and_then(|other_session|{
            // other_session.events().for_each(|event|{
            //     match event{
            //         Ok(ProfileEvent::PairingRequest(half_proof))=>{
            //             Box::new(other_gateway.pair_response(
            //                 Relation::new(
            //                     &other_profile,
            //                     &RelationProof::from_halfproof(half_proof.clone(), other_gateway.signer.sign(&[111,123,143])))
            //             ).map_err(|_|())) as Box<Future<Item=(),Error = ()> >
            //         },
            //         _=>Box::new(future::ok(()))
            //     }
            // }).map_err(|_|ErrorToBeSpecified::TODO(String::from("pairing response.fail")))
            println!("user_two_events"); 
            let events = other_session.events();
            events.take(1).collect()
            .map_err(|_|ErrorToBeSpecified::TODO(String::from("pairing response.fail")))
            .and_then(|first|{
                println!("user_two_gets_request");
                let event = &first[0];
                match event{
                    &Ok(ProfileEvent::PairingRequest(ref half_proof))=>{
                        //TODO should look something like gateway.accept(half_proof)
                        Box::new(other_gateway.pair_response(
                                Relation::new(
                                &other_profile,
                                &RelationProof::from_halfproof(half_proof.clone(), other_gateway.signer.sign("apples".as_bytes())))
                        ))
                    },
                    _=>panic!("ProfileEvent assert fail")
                }
            })
            .and_then(move |_|{
                println!("user_two_checks_into_app");
                other_session.checkin_app( &ApplicationId( String::from( "SampleApp" ) ) )
                    .take(1).collect().map_err(|_e|ErrorToBeSpecified::TODO(String::from("Test error n+1")))
            })
        })
        .and_then(|calls|{
            for call in calls{
                let incall = call.unwrap();
                let ptr = incall.request_details();

                let sink = ptr.to_caller.to_owned().unwrap();
                let sent =sink.send(Ok(AppMessageFrame(Vec::from("sink.send"))));
                println!("{:?}", sent);
                //incall.answer(None);
            }
            futures::future::ok(()) 
        });  

        let joined_f4t = Future::join(sess, other_reg); 
        let _definitive_success = reactor.run(joined_f4t);
        println!( "***We're done here, let's go packing" );
    }
}