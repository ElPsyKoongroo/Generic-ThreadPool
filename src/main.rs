use std::marker::PhantomData;
use std::sync::Condvar;
use std::sync::Arc;
use std::sync::Mutex;
use std::collections::VecDeque;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::channel;


trait FnBox<T: 'static + Send + Sized> {
    fn call_box(self: Box<Self>) -> T;
}


// Implement FnBox to every FnOnce that returns T
// T being 'static + Send 
impl<T: 'static + Send, F: FnOnce() -> T> FnBox<T> for F {
    fn call_box(self: Box<F>) -> T {
        (*self)()
    }
}


// Struct that holds a Box to a FnBox 
struct BoxedFn<T: 'static + Send + Sized> {
    fun: Box<dyn FnBox<T>>,
    _p: PhantomData<T>
}

impl<T: 'static + Send + Sized> BoxedFn<T> {

    fn new(f: Box<dyn FnBox<T>>) -> BoxedFn<T> {
        BoxedFn { fun: f, _p: PhantomData }
    }

    fn call(self) -> T {
        self.fun.call_box()
    }
}

unsafe impl<T> std::marker::Send for BoxedFn<T> 
where T: 'static + Send {
    
}




struct Executor <Q: 'static + Send> {
    stop : bool,
    tasks: VecDeque<BoxedFn<Q>>,
    _a: PhantomData<Q>
}


impl<T: 'static + Send> Executor<T> {
    pub fn new() -> Executor<T> {
        Executor { stop: false, tasks: VecDeque::new(), _a: PhantomData}
    }
}


// impl<T: 'static + Send, F: 'static + Send + FnOnce() -> T> Executor<T, F> {
//     pub fn new() -> Executor<T, F> {
//         Executor { stop: false, tasks: VecDeque::new(), a: PhantomData}
//     }
// }


struct ThreadPool<T: 'static + Send> {
    max_running_threads: usize,
    waiting_threads: Vec<std::thread::Thread>,
    free_threads: Vec<usize>,
    task_aviliable: Arc<Condvar>,
    tasks_to_execute: Arc<Mutex<Executor<T>>>,
    reciver_index_cond: Arc<Condvar>,
    reciver_index: Arc<Mutex<isize>>,
    recivers: Vec<std::sync::mpsc::Receiver<T>>,
    _p: PhantomData<T>
}


impl<T: 'static + Send> ThreadPool<T> {
    
    pub fn new_thread_pool(max_running_threads: usize) -> ThreadPool<T> {
        let mut threads = Vec::with_capacity(max_running_threads);
        let tasks_to_execute: Arc<Mutex<Executor<T>>> = Arc::new(
        Mutex::new(
                Executor::new()
        ));

        let cond = Arc::new(Condvar::new());
        let reciver_index_cond = Arc::new(Condvar::new());
        let reciver_index = Arc::new(Mutex::new(-1 as isize));
        let mut recivers = Vec::new();

        for id in 0..(max_running_threads as isize) {
            let pair2 = (Arc::clone(&tasks_to_execute), Arc::clone(&cond));
            let (sender, reciver) = channel();

            let recv_index_cond = Arc::clone(&reciver_index_cond);
            let reciver_index = Arc::clone(&reciver_index);

            recivers.push(reciver);
            let thread = std::thread::Builder::new().spawn( 
                move || {
                    let (lock, cvar) = pair2;
                    loop {
                        let mut queue = lock.lock().unwrap();
                        
                        queue = cvar.wait_while(
                            queue,
                            |colita| { colita.tasks.is_empty() || colita.stop } 
                        ).unwrap();
            
                        if queue.stop {
                            println!("Cague");
                            return ;
                        }
                        // println!("Thread {} ha cogido la tarea", id);
                        let function = queue.tasks.pop_front().unwrap().call();

                        let mut reciver_index_guard = reciver_index.lock().unwrap();
                        reciver_index_guard = recv_index_cond.wait_while(reciver_index_guard, |value| *value != -1).unwrap();
                        *reciver_index_guard = id;
                        recv_index_cond.notify_all();
                        
                        drop(queue);
                        sender.send(function).unwrap(); 
                    }
            });
            threads.push(thread);
                
        };

        ThreadPool { 
            max_running_threads, 
            waiting_threads: Vec::with_capacity(max_running_threads), 
            free_threads: (0..max_running_threads).collect(),
            task_aviliable: cond,
            tasks_to_execute,
            recivers,
            reciver_index_cond,
            reciver_index,
            _p: PhantomData
        }
    }
    /*
        enq()   -Notifica una thread y se queda esperando-> T1
        T1      -Pone su numerito en el index o espera mientras alguien mas lo pone.
        T1      -Notifica de que ha puesto el numero a-> enq()
        enq()   -Devuelve el numero al usuario
    */
    
    pub fn enqueue(&mut self, f: BoxedFn<T>) -> usize {
        
        // Añadimos la tarea a la cola
        let mut tasks = self.tasks_to_execute.lock().unwrap();
        tasks.tasks.push_back(f);
        drop(tasks);

        // println!("Tarea añadida");

        // Avisamos de que hay una tarea disponible
        self.task_aviliable.notify_one();

        // println!("Threads notificadas");

        // Esperamos a que la thread ponga su id en index_guard
        let mut index_guard = self.reciver_index.lock().unwrap();
        // println!("Esperando id de la thread");
        index_guard = self.reciver_index_cond.wait_while(index_guard, |index| *index == -1).unwrap();
        // println!("Id recivida");

        // Cogemos el valor de index_guard y lo liberamos
        let real_index = *index_guard;
        *index_guard = -1;
        drop(index_guard);

        // Devolvemos el canal correspondiente
        return real_index as usize;
        //&self.recivers[real_index as usize]
    }


    pub fn get_reciver(&self, index: usize) -> &Receiver<T> {
        return &self.recivers[index]
    }

    pub fn shutdown(&self){
        

    }

}


fn main() { 
    
    let mut thread_pool  = ThreadPool::new_thread_pool(3);


    let msgs = vec![
        "Hola" ,
        "Hola bb",
        "Hola joaquin",
        "Hola juan",
        "Hola pipo",
        "Calculando", 
        "Viva Rust",
        "Alcachofa"
    ];


    let mut ids = Vec::new();
    let mut functions = Vec::new();

    for (i, msg) in msgs.clone().into_iter().enumerate() {
        let function = move || {println!("{}",msg); i};
        functions.push(BoxedFn::new(Box::new(function)));
    }

    for function in functions.into_iter() {
        let id = thread_pool.enqueue(function);
        ids.push(id)
    }

    for id in ids {
        let res = thread_pool.get_reciver(id);
        println!("{:?}", res.recv());
    }

    // =================================================================================

    functions = Vec::new();
    ids = Vec::new();
    

    for (i, msg) in msgs.clone().into_iter().enumerate() {
        let function = move || {println!("{}",msg); i};
        functions.push(BoxedFn::new(Box::new(function)));
    }

    for function in functions.into_iter() {
        let id = thread_pool.enqueue(function);
        ids.push(id)
    }

    for id in ids {
        let res = thread_pool.get_reciver(id);
        println!("{:?}", res.recv());
    }

}

